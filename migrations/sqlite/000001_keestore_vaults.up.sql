-- SQLite. `keestore` is an ATTACHed database file, attached on every pooled
-- connection by kubuno-db, so the qualified names below resolve as they do on
-- the other two engines.
--
-- Differences from the PostgreSQL file, and why:
--   * UUID -> BLOB: what sqlx encodes a `uuid::Uuid` as on SQLite.
--   * No DEFAULT on `id`: SQLite has no UUID generator, and the process
--     supplies the key.
--   * TIMESTAMPTZ -> TEXT, in the `%F %T%.f` shape sqlx decodes into
--     DateTime<Utc>. Every value written is UTC.
--   * The updated_at trigger is written by hand; it does not recurse because
--     SQLite leaves recursive_triggers off.
CREATE TABLE keestore.vaults (
    id               BLOB    NOT NULL PRIMARY KEY,
    owner_id         BLOB    NOT NULL UNIQUE,
    kdbx_path        TEXT    NOT NULL,
    file_size_bytes  INTEGER NOT NULL DEFAULT 0,
    sync_version     INTEGER NOT NULL DEFAULT 0,
    file_hash_sha256 TEXT,
    last_accessed_at TEXT,
    last_modified_at TEXT,
    unlock_attempts  INTEGER NOT NULL DEFAULT 0,
    locked_until     TEXT,
    created_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    updated_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);

CREATE INDEX keestore.idx_ks_vaults_owner ON vaults(owner_id);

CREATE TRIGGER keestore.vaults_updated_at AFTER UPDATE ON vaults
BEGIN
    UPDATE vaults SET updated_at = strftime('%Y-%m-%d %H:%M:%f', 'now') WHERE id = NEW.id;
END;
