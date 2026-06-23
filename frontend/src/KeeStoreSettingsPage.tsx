import React, { useState } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { ArrowLeft, ExternalLink, Check, Settings } from 'lucide-react'
import { Toggle, Button, Radio } from '@ui'
import KeeStoreLogo from './KeeStoreLogo'
import { useModulePrefs } from './userPrefs'

// ── Per-user preferences (backend, cross-device via core users.preferences) ─────
//
// SECURITY: this page stores only display/behaviour preferences. It NEVER
// reads, displays, logs or persists any vault password or secret. The vault
// itself is decrypted client-side and never leaves the device unencrypted.

interface KeestorePrefs {
  autoLockMin:   string   // '1' | '5' | '15' — auto-lock idle delay (minutes)
  genLength:     string   // '12' | '16' | '20' | '32' — default generator length
  maskPasswords: boolean  // hide passwords by default in the list
  clipboardSecs: string   // '0' | '15' | '30' | '60' — clear clipboard after N seconds
  defaultView:   string   // 'list' | 'grid'
  sort:          string   // 'title' | 'username' | 'recent'
}

const DEFAULT_PREFS: KeestorePrefs = {
  autoLockMin: '5', genLength: '16', maskPasswords: true,
  clipboardSecs: '30', defaultView: 'list', sort: 'title',
}

// ── Mail-style layout helpers ───────────────────────────────────────────────────

function SettingsRow({ label, description, children }: {
  label: string; description?: string; children: React.ReactNode
}) {
  return (
    <div className="flex items-start gap-8 py-4 border-b border-[#e8eaed] last:border-0">
      <div className="w-60 flex-shrink-0">
        <p className="text-sm text-[#202124] font-normal">{label}</p>
        {description && <p className="text-xs text-text-tertiary mt-0.5 leading-relaxed">{description}</p>}
      </div>
      <div className="flex-1">{children}</div>
    </div>
  )
}

function RadioGroup({ options, value, onChange }: {
  options: { value: string; label: string }[]; value: string; onChange: (v: string) => void
}) {
  return (
    <div className="flex flex-col items-start gap-2">
      {options.map(opt => (
        <Radio key={opt.value} checked={value === opt.value} onChange={() => onChange(opt.value)} label={opt.label} />
      ))}
    </div>
  )
}

// ── Préférences tab (per-user) ──────────────────────────────────────────────────

function PreferencesTab() {
  const { t } = useTranslation('keestore')
  const { prefs: saved, update } = useModulePrefs<KeestorePrefs>('keestore', DEFAULT_PREFS)
  const [prefs, setPrefs] = useState<KeestorePrefs>(saved)
  const [savedFlag, setSavedFlag] = useState(false)
  const [busy, setBusy] = useState(false)

  const set = <K extends keyof KeestorePrefs>(key: K, value: KeestorePrefs[K]) =>
    setPrefs(p => ({ ...p, [key]: value }))

  const save = async () => {
    setBusy(true)
    try {
      await update(prefs)
      setSavedFlag(true)
      setTimeout(() => setSavedFlag(false), 2500)
    } finally { setBusy(false) }
  }

  return (
    <div>
      <SettingsRow
        label={t('kee_pref_autolock', { defaultValue: 'Verrouillage automatique' })}
        description={t('kee_pref_autolock_desc', { defaultValue: 'Reverrouiller le coffre après une période d\'inactivité.' })}
      >
        <RadioGroup
          value={prefs.autoLockMin}
          onChange={v => set('autoLockMin', v)}
          options={[
            { value: '1',  label: t('kee_pref_minutes', { defaultValue: '{{count}} minute', count: 1 }) },
            { value: '5',  label: t('kee_pref_minutes', { defaultValue: '{{count}} minutes', count: 5 }) },
            { value: '15', label: t('kee_pref_minutes', { defaultValue: '{{count}} minutes', count: 15 }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('kee_pref_genlength', { defaultValue: 'Longueur du générateur' })}
        description={t('kee_pref_genlength_desc', { defaultValue: 'Nombre de caractères par défaut des mots de passe générés.' })}
      >
        <RadioGroup
          value={prefs.genLength}
          onChange={v => set('genLength', v)}
          options={[
            { value: '12', label: t('kee_pref_chars', { defaultValue: '{{count}} caractères', count: 12 }) },
            { value: '16', label: t('kee_pref_chars', { defaultValue: '{{count}} caractères', count: 16 }) },
            { value: '20', label: t('kee_pref_chars', { defaultValue: '{{count}} caractères', count: 20 }) },
            { value: '32', label: t('kee_pref_chars', { defaultValue: '{{count}} caractères', count: 32 }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('kee_pref_clipboard', { defaultValue: 'Effacement du presse-papier' })}
        description={t('kee_pref_clipboard_desc', { defaultValue: 'Effacer le presse-papier après avoir copié un mot de passe.' })}
      >
        <RadioGroup
          value={prefs.clipboardSecs}
          onChange={v => set('clipboardSecs', v)}
          options={[
            { value: '15', label: t('kee_pref_seconds', { defaultValue: '{{count}} secondes', count: 15 }) },
            { value: '30', label: t('kee_pref_seconds', { defaultValue: '{{count}} secondes', count: 30 }) },
            { value: '60', label: t('kee_pref_seconds', { defaultValue: '{{count}} secondes', count: 60 }) },
            { value: '0',  label: t('kee_pref_clipboard_never', { defaultValue: 'Ne jamais effacer' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('kee_pref_view', { defaultValue: 'Vue par défaut' })}
      >
        <RadioGroup
          value={prefs.defaultView}
          onChange={v => set('defaultView', v)}
          options={[
            { value: 'list', label: t('kee_pref_view_list', { defaultValue: 'Liste' }) },
            { value: 'grid', label: t('kee_pref_view_grid', { defaultValue: 'Grille' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow label={t('kee_pref_sort', { defaultValue: 'Tri par défaut' })}>
        <RadioGroup
          value={prefs.sort}
          onChange={v => set('sort', v)}
          options={[
            { value: 'title',    label: t('kee_pref_sort_title',    { defaultValue: 'Titre (A→Z)' }) },
            { value: 'username', label: t('kee_pref_sort_username', { defaultValue: 'Nom d\'utilisateur' }) },
            { value: 'recent',   label: t('kee_pref_sort_recent',   { defaultValue: 'Modifié récemment' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('kee_pref_mask', { defaultValue: 'Masquage des mots de passe' })}
        description={t('kee_pref_mask_desc', { defaultValue: 'Afficher les mots de passe masqués jusqu\'à ce que vous cliquiez sur l\'icône.' })}
      >
        <label className="flex items-center gap-2 cursor-pointer select-none">
          <Toggle checked={prefs.maskPasswords} onChange={() => set('maskPasswords', !prefs.maskPasswords)} />
          <span className="text-sm text-text-primary">{t('kee_pref_mask_on', { defaultValue: 'Masquer les mots de passe par défaut' })}</span>
        </label>
      </SettingsRow>

      <div className="pt-5 flex items-center gap-3">
        <Button onClick={save} loading={busy}>
          {savedFlag
            ? <><Check size={14} className="mr-1.5 inline" />{t('kee_settings_saved', { defaultValue: 'Enregistré' })}</>
            : t('kee_settings_save_changes', { defaultValue: 'Enregistrer les modifications' })}
        </Button>
        <Button variant="ghost" onClick={() => setPrefs(saved)}>
          {t('common_cancel', { defaultValue: 'Annuler' })}
        </Button>
      </div>
    </div>
  )
}

// ── À propos tab ─────────────────────────────────────────────────────────────────

function AboutTab() {
  const { t } = useTranslation('keestore')
  return (
    <div className="rounded-xl border border-border overflow-hidden">
      <div className="flex items-center gap-3 px-5 py-4 border-b border-border bg-surface-1">
        <div className="w-10 h-10 rounded-xl bg-indigo-100 flex items-center justify-center shrink-0">
          <KeeStoreLogo size={22} />
        </div>
        <div>
          <p className="text-sm font-semibold text-text-primary">Kubuno Keestore</p>
          <p className="text-xs text-text-tertiary">v0.1.0 · {t('kee_official_module', { defaultValue: 'Module officiel' })}</p>
        </div>
        <span className="ml-auto text-xs font-medium px-2 py-0.5 rounded-full bg-orange-100 text-orange-700">Rust</span>
      </div>
      <div className="px-5 py-4 space-y-3">
        <p className="text-xs text-text-tertiary leading-relaxed">
          {t('kee_about_desc', { defaultValue: 'Gestionnaire de mots de passe compatible KeePass. Le coffre est chiffré et déchiffré dans votre navigateur ; aucun secret n\'est jamais transmis en clair.' })}
        </p>
        <a href="https://github.com/kubuno/keestore" target="_blank" rel="noopener noreferrer"
          className="inline-flex items-center gap-1.5 text-sm text-primary hover:underline">
          <ExternalLink size={13} /> github.com/kubuno/keestore
        </a>
      </div>
    </div>
  )
}

// ── Main page (mail-style breadcrumb + tab bar) ─────────────────────────────────

type Tab = 'preferences' | 'about'

export default function KeeStoreSettingsPage() {
  const { t } = useTranslation('keestore')
  const [tab, setTab] = useState<Tab>('preferences')

  const tabs: { id: Tab; label: string }[] = [
    { id: 'preferences', label: t('kee_tab_preferences', { defaultValue: 'Préférences' }) },
    { id: 'about',       label: t('kee_tab_about', { defaultValue: 'À propos' }) },
  ]

  return (
    <div className="flex flex-col h-full bg-white overflow-hidden">
      {/* Breadcrumb header */}
      <div className="flex items-center gap-2 px-6 py-2.5 border-b border-[#e8eaed] flex-shrink-0" style={{ background: '#f8f9fa' }}>
        <Link to="/keestore" className="flex items-center gap-1.5 text-sm text-[#1a73e8] hover:underline">
          <ArrowLeft size={14} />
          Keestore
        </Link>
        <span className="text-text-tertiary text-sm">/</span>
        <div className="flex items-center gap-1.5">
          <Settings size={15} className="text-text-secondary" />
          <span className="text-sm text-text-primary">{t('kee_settings_title', { defaultValue: 'Réglages' })}</span>
        </div>
      </div>

      {/* Tab bar (Gmail-style) */}
      <div className="flex items-end border-b border-[#e8eaed] px-4 flex-shrink-0 overflow-x-auto" style={{ background: '#fff' }}>
        {tabs.map(tb => (
          <button key={tb.id} onClick={() => setTab(tb.id)}
            className={`px-4 py-3 text-sm border-b-2 -mb-px transition-colors whitespace-nowrap ${
              tab === tb.id ? 'border-[#1a73e8] text-[#1a73e8] font-medium' : 'border-transparent text-[#5f6368] hover:text-[#202124] hover:bg-[#f1f3f4]'}`}>
            {tb.label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-3xl mx-auto px-8 py-6">
          {tab === 'preferences' && <PreferencesTab />}
          {tab === 'about'       && <AboutTab />}
        </div>
      </div>
    </div>
  )
}
