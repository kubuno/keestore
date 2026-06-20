import { useState, useCallback, useRef, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import * as kdbxweb from 'kdbxweb'
import {
  KeyRound, Upload, Lock, Unlock, Search, Copy, Eye, EyeOff,
  Globe, User, FileText, ChevronRight, Shield, ShieldAlert,
  ShieldCheck, RefreshCw, Trash2, CloudUpload, FolderOpen,
  AlertCircle, CheckCircle2, X,
} from 'lucide-react'
import { api } from '@kubuno/sdk'
import { Button, Input } from '@ui'
import { ConfirmDialog } from '@ui'
import { useConfirm } from '@kubuno/sdk'
import { getDateLocale } from '@kubuno/sdk'
import { format } from 'date-fns'
import { argon2d, argon2id } from 'hash-wasm'
import { WorkspaceShell, WORKSPACE_LIGHT } from '@kubuno/sdk'

// kdbxweb NE FOURNIT PAS d'implémentation Argon2 : les coffres KDBX4 utilisent Argon2
// comme KDF, donc sans cet enregistrement `Kdbx.load` lève « argon2 not implemented ».
// On la câble via hash-wasm (WASM, exécuté côté client → zéro-connaissance préservé).
// kdbxweb passe `memory` déjà en KiB (header/1024) ; `type` 0 = Argon2d, 2 = Argon2id.
kdbxweb.CryptoEngine.setArgon2Impl(async (password, salt, memory, iterations, length, parallelism, type) => {
  const hashFn = type === 2 ? argon2id : argon2d
  const hash = await hashFn({
    password:    new Uint8Array(password),
    salt:        new Uint8Array(salt),
    parallelism,
    iterations,
    memorySize:  memory,   // KiB
    hashLength:  length,
    outputType:  'binary',
  })
  return hash.buffer as ArrayBuffer
})

// ── Types ─────────────────────────────────────────────────────────────────────

interface VaultStatus {
  exists:           boolean
  sync_version:     number
  file_size_bytes:  number
  last_modified_at: string | null
}

interface Entry {
  uuid:      string
  title:     string
  username:  string
  password:  string
  url:       string
  notes:     string
  groupPath: string
}

type PageState = 'loading' | 'no-vault' | 'locked' | 'unlocked'

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtBytes(b: number) {
  if (b < 1024) return `${b} B`
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`
  return `${(b / (1024 * 1024)).toFixed(2)} MB`
}

function fmtDate(s: string | null, lng?: string) {
  if (!s) return '—'
  return format(new Date(s), 'dd MMM yyyy HH:mm', { locale: getDateLocale(lng) })
}

function getField(entry: kdbxweb.KdbxEntry, key: string): string {
  const val = entry.fields.get(key)
  if (!val) return ''
  if (val instanceof kdbxweb.ProtectedValue) return val.getText()
  return String(val)
}

function walkGroup(group: kdbxweb.KdbxGroup, path: string, out: Entry[]) {
  const groupPath = path ? `${path} / ${group.name}` : (group.name ?? '')
  for (const entry of group.entries) {
    out.push({
      uuid:      entry.uuid.id,
      title:     getField(entry, 'Title'),
      username:  getField(entry, 'UserName'),
      password:  getField(entry, 'Password'),
      url:       getField(entry, 'URL'),
      notes:     getField(entry, 'Notes'),
      groupPath,
    })
  }
  for (const sub of group.groups) {
    walkGroup(sub, groupPath, out)
  }
}

async function hibpCount(password: string): Promise<number> {
  const buf    = new TextEncoder().encode(password)
  const digest = await crypto.subtle.digest('SHA-1', buf)
  const hex    = Array.from(new Uint8Array(digest))
    .map(b => b.toString(16).padStart(2, '0')).join('').toUpperCase()
  const prefix = hex.slice(0, 5)
  const suffix = hex.slice(5)
  try {
    const { data } = await api.get<string>(`/keestore/hibp/${prefix}`, { responseType: 'text' })
    for (const line of data.split('\n')) {
      const [h, c] = line.split(':')
      if (h?.trim() === suffix) return parseInt(c ?? '0')
    }
  } catch {/* réseau */}
  return 0
}

// ── Copy button ───────────────────────────────────────────────────────────────

function CopyBtn({ value, title }: { value: string; title: string }) {
  const [copied, setCopied] = useState(false)
  const copy = () => {
    navigator.clipboard.writeText(value)
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }
  return (
    <button
      onClick={copy}
      title={title}
      className="p-1 rounded text-text-tertiary hover:text-primary hover:bg-primary/10 transition-colors"
    >
      {copied ? <CheckCircle2 size={14} className="text-success" /> : <Copy size={14} />}
    </button>
  )
}

// ── No vault panel ────────────────────────────────────────────────────────────

function NoVaultPanel({ onUploaded }: { onUploaded: () => void }) {
  const { t } = useTranslation('keestore')
  const inputRef   = useRef<HTMLInputElement>(null)
  const [uploading, setUploading] = useState(false)
  const [error, setError]         = useState<string | null>(null)
  const [dragOver, setDragOver]   = useState(false)

  const upload = useCallback(async (file: File) => {
    setError(null)
    setUploading(true)
    try {
      const buf = await file.arrayBuffer()
      const magic = new Uint8Array(buf).slice(0, 4)
      if (!(magic[0] === 0x03 && magic[1] === 0xD9 && magic[2] === 0xA2 && magic[3] === 0x9A)) {
        setError(t('kee_err_not_kdbx'))
        return
      }
      await api.put('/keestore/kdbx', buf, { headers: { 'Content-Type': 'application/octet-stream' } })
      onUploaded()
    } catch {
      setError(t('kee_err_upload_failed'))
    } finally {
      setUploading(false)
    }
  }, [onUploaded, t])

  const onDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    setDragOver(false)
    const f = e.dataTransfer.files[0]
    if (f) upload(f)
  }, [upload])

  return (
    <div className="flex flex-col items-center justify-center min-h-[60vh] gap-6 p-8">
      <div className="w-20 h-20 rounded-2xl bg-primary/10 flex items-center justify-center">
        <KeyRound size={36} className="text-primary" />
      </div>
      <div className="text-center">
        <h1 className="text-xl font-semibold text-text-primary mb-1">{t('kee_no_vault_title')}</h1>
        <p className="text-sm text-text-secondary max-w-sm">
          {t('kee_no_vault_desc_before')}<code className="font-mono bg-surface-2 px-1 rounded">.kdbx</code>{t('kee_no_vault_desc_after')}
        </p>
      </div>

      <div
        onDrop={onDrop}
        onDragOver={e => { e.preventDefault(); setDragOver(true) }}
        onDragLeave={() => setDragOver(false)}
        onClick={() => inputRef.current?.click()}
        className={`w-full max-w-sm border-2 border-dashed rounded-xl p-8 text-center cursor-pointer transition-colors ${
          dragOver ? 'border-primary bg-primary/5' : 'border-border hover:border-primary/50 hover:bg-surface-1'
        }`}
      >
        <input
          ref={inputRef} type="file" accept=".kdbx" className="hidden"
          onChange={e => { const f = e.target.files?.[0]; if (f) upload(f) }}
        />
        <Upload size={24} className="mx-auto mb-2 text-text-tertiary" />
        <p className="text-sm font-medium text-text-primary">
          {uploading ? t('kee_uploading') : t('kee_drop_or_click')}
        </p>
        <p className="text-xs text-text-tertiary mt-1">{t('kee_kdbx_hint')}</p>
      </div>

      {error && (
        <div className="flex items-center gap-2 text-danger text-sm">
          <AlertCircle size={16} /> {error}
        </div>
      )}
    </div>
  )
}

// ── Unlock panel ──────────────────────────────────────────────────────────────

function UnlockPanel({
  status, onUnlocked, onDeleted,
}: {
  status: VaultStatus
  onUnlocked: (entries: Entry[]) => void
  onDeleted:  () => void
}) {
  const { t, i18n } = useTranslation('keestore')
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const [password, setPassword] = useState('')
  const [showPwd, setShowPwd]   = useState(false)
  const [loading,  setLoading]  = useState(false)
  const [error,    setError]    = useState<string | null>(null)
  const [syncing,  setSyncing]  = useState(false)
  const fileRef = useRef<HTMLInputElement>(null)

  const unlock = async () => {
    if (!password) return
    setError(null)
    setLoading(true)
    try {
      const resp = await api.get('/keestore/kdbx', { responseType: 'arraybuffer' })
      const creds = new kdbxweb.Credentials(kdbxweb.ProtectedValue.fromString(password))
      const db    = await kdbxweb.Kdbx.load(resp.data as ArrayBuffer, creds)
      const out: Entry[] = []
      if (db.getDefaultGroup()) walkGroup(db.getDefaultGroup(), '', out)
      onUnlocked(out)
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e)
      if (msg.includes('Invalid key') || msg.includes('HMAC') || msg.includes('credentials')) {
        setError(t('kee_err_wrong_password'))
      } else {
        setError(t('kee_err_open_failed', { message: msg }))
      }
    } finally {
      setLoading(false)
    }
  }

  const syncNew = async (file: File) => {
    setSyncing(true)
    try {
      const buf = await file.arrayBuffer()
      await api.put('/keestore/kdbx', buf, {
        headers: {
          'Content-Type': 'application/octet-stream',
          'X-Sync-Version': String(status.sync_version),
        },
      })
      window.location.reload()
    } catch {
      setError(t('kee_err_sync_failed'))
    } finally {
      setSyncing(false)
    }
  }

  const deleteVault = async () => {
    const ok = await confirm({
      title:        t('kee_delete_vault_title'),
      message:      t('kee_delete_vault_message'),
      confirmLabel: t('common_delete'),
      cancelLabel:  t('common_cancel'),
      variant:      'danger',
    })
    if (!ok) return
    await api.delete('/keestore/kdbx')
    onDeleted()
  }

  return (
    <div className="flex flex-col items-center justify-center min-h-[60vh] gap-6 p-8">
      <div className="w-16 h-16 rounded-2xl bg-primary/10 flex items-center justify-center">
        <Lock size={28} className="text-primary" />
      </div>
      <div className="text-center">
        <h1 className="text-lg font-semibold text-text-primary mb-1">{t('kee_vault_locked')}</h1>
        <div className="flex items-center justify-center gap-4 text-xs text-text-tertiary mt-1">
          <span>{fmtBytes(status.file_size_bytes)}</span>
          <span>·</span>
          <span>v{status.sync_version}</span>
          <span>·</span>
          <span>{fmtDate(status.last_modified_at, i18n.language)}</span>
        </div>
      </div>

      <div className="w-full max-w-xs flex flex-col gap-3">
        <div className="relative">
          <Input
            type={showPwd ? 'text' : 'password'}
            value={password}
            onChange={e => setPassword(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && unlock()}
            placeholder={t('kee_master_password')}
            autoFocus
            className="pr-9"
          />
          <button
            type="button"
            onClick={() => setShowPwd(p => !p)}
            className="absolute right-2.5 top-1/2 -translate-y-1/2 text-text-tertiary hover:text-text-primary"
          >
            {showPwd ? <EyeOff size={15} /> : <Eye size={15} />}
          </button>
        </div>

        <Button
          className="w-full"
          icon={<Unlock size={15} />}
          onClick={unlock}
          disabled={!password}
          loading={loading}
        >
          {loading ? t('kee_decrypting') : t('kee_unlock')}
        </Button>

        {error && (
          <div className="flex items-center gap-2 text-danger text-sm">
            <AlertCircle size={15} /> {error}
          </div>
        )}
      </div>

      <div className="flex items-center gap-3 text-xs text-text-tertiary">
        <button
          onClick={() => fileRef.current?.click()}
          className="flex items-center gap-1 hover:text-primary transition-colors"
        >
          <CloudUpload size={13} />
          {syncing ? t('kee_syncing') : t('kee_sync_new_version')}
        </button>
        <input
          ref={fileRef} type="file" accept=".kdbx" className="hidden"
          onChange={e => { const f = e.target.files?.[0]; if (f) syncNew(f) }}
        />
        <span>·</span>
        <button
          onClick={deleteVault}
          className="flex items-center gap-1 hover:text-danger transition-colors"
        >
          <Trash2 size={13} /> {t('common_delete')}
        </button>
      </div>

      {confirmState && (
        <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
      )}
    </div>
  )
}

// ── Entry detail ──────────────────────────────────────────────────────────────

function EntryDetail({ entry, onClose }: { entry: Entry; onClose: () => void }) {
  const { t, i18n } = useTranslation('keestore')
  const [showPwd, setShowPwd]   = useState(false)
  const [hibp,    setHibp]      = useState<number | null>(null)
  const [checking, setChecking] = useState(false)

  const checkHibp = async () => {
    if (!entry.password) return
    setChecking(true)
    const n = await hibpCount(entry.password)
    setHibp(n)
    setChecking(false)
  }

  return (
    <div className="flex flex-col h-full border-l border-border bg-surface-0">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <div className="flex items-center gap-2 min-w-0">
          <div className="w-8 h-8 rounded-lg bg-primary/10 flex items-center justify-center flex-shrink-0">
            <KeyRound size={16} className="text-primary" />
          </div>
          <div className="min-w-0">
            <p className="text-sm font-medium text-text-primary truncate">{entry.title || t('kee_untitled_entry')}</p>
            <p className="text-xs text-text-tertiary truncate">{entry.groupPath}</p>
          </div>
        </div>
        <button onClick={onClose} className="p-1 rounded hover:bg-surface-2 text-text-tertiary hover:text-text-primary no-print">
          <X size={16} />
        </button>
      </div>

      {/* Fields */}
      <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-4">
        {entry.username && (
          <Field label={t('kee_field_username')} icon={<User size={14} />} value={entry.username} copyTitle={t('kee_copy_username')} copy />
        )}

        {entry.password && (
          <div>
            <label className="flex items-center gap-1.5 text-xs font-medium text-text-secondary mb-1.5">
              <Lock size={14} /> {t('kee_field_password')}
            </label>
            {/* Sécurité : le mot de passe (en clair si révélé) ne doit JAMAIS être
                imprimé → la boîte de valeur est entièrement non imprimable. */}
            <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-surface-1 border border-border no-print">
              <span className="flex-1 font-mono text-sm text-text-primary select-all">
                {showPwd ? entry.password : '•'.repeat(Math.min(entry.password.length, 20))}
              </span>
              <button onClick={() => setShowPwd(p => !p)} className="text-text-tertiary hover:text-primary">
                {showPwd ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
              <CopyBtn value={entry.password} title={t('kee_copy_password')} />
            </div>
            <span className="print-only text-sm text-text-tertiary italic">{'••••••••'}</span>

            {/* HIBP */}
            <div className="mt-2 flex items-center gap-2 no-print">
              <button
                onClick={checkHibp}
                disabled={checking}
                className="flex items-center gap-1.5 text-xs text-text-tertiary hover:text-primary transition-colors disabled:opacity-50"
              >
                {checking ? <RefreshCw size={12} className="animate-spin" /> : <Shield size={12} />}
                {t('kee_check_breaches')}
              </button>
              {hibp !== null && hibp === 0 && (
                <span className="flex items-center gap-1 text-xs text-success">
                  <ShieldCheck size={12} /> {t('kee_not_compromised')}
                </span>
              )}
              {hibp !== null && hibp > 0 && (
                <span className="flex items-center gap-1 text-xs text-danger">
                  <ShieldAlert size={12} /> {t('kee_breaches_count', { count: hibp, formatted: hibp.toLocaleString(i18n.language) })}
                </span>
              )}
            </div>
          </div>
        )}

        {entry.url && (
          <div>
            <label className="flex items-center gap-1.5 text-xs font-medium text-text-secondary mb-1.5">
              <Globe size={14} /> {t('kee_field_url')}
            </label>
            <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-surface-1 border border-border">
              <a
                href={entry.url.startsWith('http') ? entry.url : `https://${entry.url}`}
                target="_blank" rel="noopener noreferrer"
                className="flex-1 text-sm text-primary hover:underline truncate"
              >
                {entry.url}
              </a>
              <CopyBtn value={entry.url} title={t('kee_copy_url')} />
            </div>
          </div>
        )}

        {entry.notes && (
          <div>
            <label className="flex items-center gap-1.5 text-xs font-medium text-text-secondary mb-1.5">
              <FileText size={14} /> {t('kee_field_notes')}
            </label>
            <p className="text-sm text-text-primary whitespace-pre-wrap bg-surface-1 border border-border rounded-lg px-3 py-2">
              {entry.notes}
            </p>
          </div>
        )}
      </div>
    </div>
  )
}

function Field({
  label, icon, value, copy: showCopy, copyTitle,
}: { label: string; icon: React.ReactNode; value: string; copy?: boolean; copyTitle?: string }) {
  return (
    <div>
      <label className="flex items-center gap-1.5 text-xs font-medium text-text-secondary mb-1.5">
        {icon} {label}
      </label>
      <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-surface-1 border border-border">
        <span className="flex-1 text-sm text-text-primary select-all">{value}</span>
        {showCopy && <CopyBtn value={value} title={copyTitle ?? label} />}
      </div>
    </div>
  )
}

// ── Entry row ─────────────────────────────────────────────────────────────────

function EntryRow({ entry, selected, onClick }: {
  entry: Entry; selected: boolean; onClick: () => void
}) {
  const { t } = useTranslation('keestore')
  return (
    <button
      onClick={onClick}
      className={`w-full flex items-center gap-3 px-3 py-2.5 text-left rounded-lg transition-colors ${
        selected ? 'bg-primary/10 text-primary' : 'hover:bg-surface-1'
      }`}
    >
      <div className={`w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0 ${
        selected ? 'bg-primary/20' : 'bg-surface-2'
      }`}>
        <KeyRound size={15} className={selected ? 'text-primary' : 'text-text-tertiary'} />
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium truncate">{entry.title || t('kee_untitled_entry')}</p>
        {entry.username && (
          <p className="text-xs text-text-tertiary truncate">{entry.username}</p>
        )}
      </div>
      <ChevronRight size={14} className="flex-shrink-0 text-text-tertiary" />
    </button>
  )
}

// ── Unlocked vault view ───────────────────────────────────────────────────────

// matchMedia (pas `md:`) : les variantes responsives d'un module qui annulent une
// classe de base courante (flex-col→md:flex-row, w-full→md:w-[280px]) sont écrasées
// par l'utilitaire de base du host (couche utilities > kubuno-module).
function useIsMobile(): boolean {
  const [m, setM] = useState(() =>
    typeof window !== 'undefined' && window.matchMedia('(max-width: 1023px)').matches)
  useEffect(() => {
    const mq = window.matchMedia('(max-width: 1023px)')
    const on = () => setM(mq.matches)
    mq.addEventListener('change', on)
    return () => mq.removeEventListener('change', on)
  }, [])
  return m
}

function VaultView({
  entries, onLock, status,
}: {
  entries: Entry[]; onLock: () => void; status: VaultStatus
}) {
  const { t, i18n } = useTranslation('keestore')
  const isMobile = useIsMobile()
  const [search,   setSearch]   = useState('')
  const [selected, setSelected] = useState<Entry | null>(null)
  const [group,    setGroup]    = useState<string | null>(null)
  const fileRef = useRef<HTMLInputElement>(null)

  const groups = Array.from(new Set(entries.map(e => e.groupPath))).filter(Boolean).sort()

  const filtered = entries.filter(e => {
    const q = search.toLowerCase()
    const matchSearch = !q || e.title.toLowerCase().includes(q)
      || e.username.toLowerCase().includes(q)
      || e.url.toLowerCase().includes(q)
    const matchGroup = !group || e.groupPath === group
    return matchSearch && matchGroup
  })

  const syncNew = async (file: File) => {
    const buf = await file.arrayBuffer()
    await api.put('/keestore/kdbx', buf, {
      headers: {
        'Content-Type':  'application/octet-stream',
        'X-Sync-Version': String(status.sync_version),
      },
    })
    onLock()
  }

  return (
    <WorkspaceShell
      theme={WORKSPACE_LIGHT}
      chromeless
      topbarHeight={64}
      onBack={onLock}
      titleIcon={<KeyRound size={16} className="text-primary flex-shrink-0" />}
      title={t('kee_title', { defaultValue: 'Keestore' })}
      subtitle={`v${status.sync_version}`}
      topbarActions={<>
        <button
          onClick={onLock}
          title={t('kee_lock')}
          className="p-1.5 rounded-lg text-text-secondary hover:text-text-primary hover:bg-surface-2 transition-colors"
        >
          <Lock size={16} />
        </button>
        <button
          onClick={() => fileRef.current?.click()}
          title={t('kee_sync_file')}
          className="p-1.5 rounded-lg text-text-secondary hover:text-text-primary hover:bg-surface-2 transition-colors"
        >
          <RefreshCw size={16} />
        </button>
        <input
          ref={fileRef} type="file" accept=".kdbx" className="hidden"
          onChange={e => { const f = e.target.files?.[0]; if (f) syncNew(f) }}
        />
      </>}
    >
    <div className={`flex ${isMobile ? 'flex-col' : 'flex-row'} flex-1 min-w-0 min-h-0 overflow-hidden`}>
      {/* Left — groups + entries (pleine largeur sur mobile, 280px sur desktop) */}
      <div className={`flex flex-col border-border flex-shrink-0 ${isMobile ? 'border-b w-full max-h-[45%]' : 'border-r w-[280px]'}`}>
        {/* Toolbar — recherche d'entrées (la recherche globale est dans la topbar) */}
        <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
          <div className="flex-1 relative">
            <Search size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-tertiary" />
            <input
              value={search}
              onChange={e => setSearch(e.target.value)}
              placeholder={t('common_search')}
              className="w-full pl-7 pr-2 py-1.5 rounded-lg bg-surface-1 border border-border text-sm focus:outline-none focus:border-primary"
            />
          </div>
        </div>

        {/* Groups */}
        {groups.length > 0 && (
          <div className="px-2 py-1.5 border-b border-border flex flex-wrap gap-1">
            <button
              onClick={() => setGroup(null)}
              className={`flex items-center gap-1 px-2 py-0.5 rounded text-xs transition-colors ${
                !group ? 'bg-primary/10 text-primary' : 'text-text-tertiary hover:bg-surface-2'
              }`}
            >
              <FolderOpen size={11} /> {t('kee_all_groups')}
            </button>
            {groups.map(g => (
              <button
                key={g}
                onClick={() => setGroup(g === group ? null : g)}
                className={`px-2 py-0.5 rounded text-xs truncate max-w-[120px] transition-colors ${
                  group === g ? 'bg-primary/10 text-primary' : 'text-text-tertiary hover:bg-surface-2'
                }`}
                title={g}
              >
                {g.split(' / ').pop()}
              </button>
            ))}
          </div>
        )}

        {/* Count */}
        <div className="px-3 py-1.5 text-xs text-text-tertiary border-b border-border">
          {t('kee_entry_count', { count: filtered.length })}
          {search && ` · "${search}"`}
        </div>

        {/* Entries */}
        <div className="flex-1 overflow-y-auto p-1.5 flex flex-col gap-0.5">
          {filtered.length === 0 ? (
            <p className="text-sm text-text-tertiary text-center py-8">{t('kee_no_results')}</p>
          ) : filtered.map(e => (
            <EntryRow
              key={e.uuid}
              entry={e}
              selected={selected?.uuid === e.uuid}
              onClick={() => setSelected(e)}
            />
          ))}
        </div>

        {/* Footer */}
        <div className="px-3 py-2 border-t border-border text-xs text-text-tertiary">
          v{status.sync_version} · {fmtDate(status.last_modified_at, i18n.language)}
        </div>
      </div>

      {/* Right — detail */}
      <div className="flex-1 overflow-hidden">
        {selected ? (
          <EntryDetail entry={selected} onClose={() => setSelected(null)} />
        ) : (
          <div className="flex flex-col items-center justify-center h-full gap-3 text-center text-text-tertiary">
            <Unlock size={32} className="opacity-30" />
            <p className="text-sm">{t('kee_select_entry')}</p>
            <p className="text-xs">{t('kee_entries_in_vault', { count: entries.length })}</p>
          </div>
        )}
      </div>
    </div>
    </WorkspaceShell>
  )
}

// ── Main export ───────────────────────────────────────────────────────────────

export default function KeeStorePage() {
  const { t } = useTranslation('keestore')
  const [pageState, setPageState] = useState<PageState>('loading')
  const [status,    setStatus]    = useState<VaultStatus | null>(null)
  const [entries,   setEntries]   = useState<Entry[]>([])

  const loadStatus = useCallback(async () => {
    try {
      const { data } = await api.get<VaultStatus>('/keestore/status')
      setStatus(data)
      setPageState(data.exists ? 'locked' : 'no-vault')
    } catch {
      setPageState('no-vault')
    }
  }, [])

  useEffect(() => { loadStatus() }, [loadStatus])

  if (pageState === 'loading') {
    return (
      <div className="flex items-center justify-center min-h-[60vh] text-text-tertiary text-sm">
        <RefreshCw size={18} className="animate-spin mr-2" /> {t('common_loading')}
      </div>
    )
  }

  if (pageState === 'no-vault') {
    return <NoVaultPanel onUploaded={loadStatus} />
  }

  if (pageState === 'locked' && status) {
    return (
      <UnlockPanel
        status={status}
        onUnlocked={es => { setEntries(es); setPageState('unlocked') }}
        onDeleted={loadStatus}
      />
    )
  }

  if (pageState === 'unlocked' && status) {
    return (
      <VaultView
        entries={entries}
        status={status}
        onLock={() => { setEntries([]); setPageState('locked') }}
      />
    )
  }

  return null
}
