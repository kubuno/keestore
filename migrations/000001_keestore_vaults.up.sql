CREATE SCHEMA IF NOT EXISTS keestore;

CREATE TABLE keestore.vaults (
    id               UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id         UUID NOT NULL UNIQUE,
    kdbx_path        TEXT NOT NULL,
    file_size_bytes  BIGINT NOT NULL DEFAULT 0,
    sync_version     BIGINT NOT NULL DEFAULT 0,
    file_hash_sha256 TEXT,
    last_accessed_at TIMESTAMPTZ,
    last_modified_at TIMESTAMPTZ,
    unlock_attempts  INTEGER NOT NULL DEFAULT 0,
    locked_until     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ks_vaults_owner ON keestore.vaults(owner_id);

CREATE OR REPLACE FUNCTION keestore.set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER vaults_updated_at
    BEFORE UPDATE ON keestore.vaults
    FOR EACH ROW EXECUTE FUNCTION keestore.set_updated_at();
