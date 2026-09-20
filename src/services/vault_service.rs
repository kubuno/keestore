use chrono::Utc;
use kubuno_db::dialect::Assign;
use kubuno_db::params;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    errors::{KeeStoreError, Result},
    models::VaultMeta,
};

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Vérifie la signature magic KDBX (0x9AA2D903 little-endian = 03 D9 A2 9A)
pub fn is_kdbx_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 4
        && bytes[0] == 0x03
        && bytes[1] == 0xD9
        && bytes[2] == 0xA2
        && bytes[3] == 0x9A
}

pub fn kdbx_path(user_id: Uuid) -> String {
    format!("keestore/{}/vault.kdbx", user_id)
}

pub async fn get_vault_meta(user_id: &Uuid, db: &kubuno_db::DbPool) -> Result<VaultMeta> {
    // The row maps straight onto VaultMeta (it derives sqlx::FromRow), which
    // decodes on the three engines, so there is no per-engine mapping here.
    db.fetch_optional_as::<VaultMeta>(
        r#"SELECT id, owner_id, kdbx_path, file_size_bytes, sync_version,
                  file_hash_sha256, last_accessed_at, last_modified_at,
                  unlock_attempts, locked_until, created_at, updated_at
           FROM keestore.vaults
           WHERE owner_id = $1"#,
        params![user_id],
    )
    .await?
    .ok_or(KeeStoreError::VaultNotFound)
}

/// Stores the vault's new metadata and returns its new sync version.
///
/// Three things in this statement are not portable, and each is handled by
/// kubuno-db rather than by hand:
///
/// * **`ON CONFLICT ... DO UPDATE`** — MySQL spells it `ON DUPLICATE KEY
///   UPDATE` and names the incoming row `VALUES(col)` instead of
///   `excluded.col`.
/// * **`RETURNING`** — MySQL has none, so the helper re-reads the row by its
///   natural key. Both statements run inside the transaction opened here, so a
///   concurrent sync cannot slip between them.
/// * **`NOW()`** — bound from Rust: the three engines spell it differently and
///   SQLite has no time zone at all.
///
/// `id` is generated here and bound explicitly. The PostgreSQL `DEFAULT` is
/// still in the (frozen) migration and simply gets overridden; MySQL and SQLite
/// have no UUID generator, and a key the process does not know cannot be read
/// back on an engine without `RETURNING`.
pub async fn sync_vault(
    db: &kubuno_db::DbPool,
    user_id: Uuid,
    kdbx_path: &str,
    file_size: i64,
    hash: &str,
) -> Result<i64> {
    let backend = db.backend();
    let insert = format!(
        "INSERT INTO keestore.vaults
           (id, owner_id, kdbx_path, file_size_bytes, sync_version,
            file_hash_sha256, last_modified_at)
           VALUES ($1, $2, $3, $4, 1, $5, $6){}",
        backend.upsert(
            "vaults",
            &["owner_id"],
            &[
                Assign::Incoming("kdbx_path"),
                Assign::Incoming("file_size_bytes"),
                Assign::Expr { col: "sync_version", expr: "{cur} + 1" },
                Assign::Incoming("file_hash_sha256"),
                Assign::Incoming("last_modified_at"),
            ],
        )
    );
    let now = Utc::now();

    let mut tx = db.begin().await?;
    let sync_version: i64 = kubuno_db::returning::insert_returning_scalar(
        &mut tx,
        &insert,
        params![kubuno_db::new_id(), user_id, kdbx_path, file_size, hash, now],
        "sync_version",
        "SELECT sync_version FROM keestore.vaults WHERE owner_id = $1",
        params![user_id],
    )
    .await?;
    tx.commit().await?;

    Ok(sync_version)
}

/// Marks the vault as read. Best-effort: a failure here must not stop a
/// download.
pub async fn touch_last_accessed(db: &kubuno_db::DbPool, user_id: Uuid) {
    // The timestamp comes from the process rather than the server: `NOW()` is
    // spelled differently on the three engines and SQLite has no time zone.
    if let Err(e) = db
        .execute(
            "UPDATE keestore.vaults SET last_accessed_at = $1 WHERE owner_id = $2",
            params![Utc::now(), user_id],
        )
        .await
    {
        tracing::warn!(error = %e, "Horodatage de dernier accès non enregistré");
    }
}

/// Removes the vault row. The blob itself is deleted by the caller.
pub async fn delete_vault(db: &kubuno_db::DbPool, user_id: Uuid) -> Result<()> {
    db.execute(
        "DELETE FROM keestore.vaults WHERE owner_id = $1",
        params![user_id],
    )
    .await?;
    Ok(())
}
