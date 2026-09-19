-- MySQL / MariaDB. The `keestore` database is created by kubuno-db's
-- `ensure_schema` before the migrator runs, so there is no CREATE here.
--
-- Differences from the PostgreSQL file, and why:
--   * UUID    -> BINARY(16): what sqlx encodes a `uuid::Uuid` as on MySQL.
--   * No DEFAULT on `id`: MySQL has no gen_random_uuid(), and the process has
--     to know the key anyway since MySQL has no RETURNING.
--   * TIMESTAMPTZ -> DATETIME(6): MySQL has no time zone in the column; every
--     value written is UTC, as produced by chrono.
--   * The updated_at trigger becomes ON UPDATE CURRENT_TIMESTAMP(6).
CREATE TABLE keestore.vaults (
    id               BINARY(16)  NOT NULL PRIMARY KEY,
    owner_id         BINARY(16)  NOT NULL UNIQUE,
    kdbx_path        TEXT        NOT NULL,
    file_size_bytes  BIGINT      NOT NULL DEFAULT 0,
    sync_version     BIGINT      NOT NULL DEFAULT 0,
    file_hash_sha256 VARCHAR(64) NULL,
    last_accessed_at DATETIME(6) NULL,
    last_modified_at DATETIME(6) NULL,
    unlock_attempts  INT         NOT NULL DEFAULT 0,
    locked_until     DATETIME(6) NULL,
    created_at       DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at       DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
                                 ON UPDATE CURRENT_TIMESTAMP(6)
);
