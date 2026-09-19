use chrono::Utc;
use kubuno_db::dialect::Assign;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{errors::{KeeStoreError, Result}, models::VaultMeta};

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

pub async fn get_vault_meta(
    user_id: &Uuid,
    db:      &kubuno_db::DbPool,
) -> Result<VaultMeta> {
    let row = kubuno_db::query(
        r#"SELECT id, owner_id, kdbx_path, file_size_bytes, sync_version,
                  file_hash_sha256, last_accessed_at, last_modified_at,
                  unlock_attempts, locked_until, created_at, updated_at
           FROM keestore.vaults
           WHERE owner_id = $1"#,
    )?
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .ok_or(KeeStoreError::VaultNotFound)?;

    Ok(VaultMeta {
        id:               row.try_get("id")?,
        owner_id:         row.try_get("owner_id")?,
        kdbx_path:        row.try_get("kdbx_path")?,
        file_size_bytes:  row.try_get("file_size_bytes")?,
        sync_version:     row.try_get("sync_version")?,
        file_hash_sha256: row.try_get("file_hash_sha256")?,
        last_accessed_at: row.try_get("last_accessed_at")?,
        last_modified_at: row.try_get("last_modified_at")?,
        unlock_attempts:  row.try_get("unlock_attempts")?,
        locked_until:     row.try_get("locked_until")?,
        created_at:       row.try_get("created_at")?,
        updated_at:       row.try_get("updated_at")?,
    })
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
/// `id` is generated here and bound explicitly. The PostgreSQL `DEFAULT
/// uuid_generate_v4()` is still in the (frozen) migration and simply gets
/// overridden; MySQL and SQLite have no UUID generator, and a key the process
/// does not know cannot be read back on an engine without `RETURNING`. On a
/// conflict the stored row keeps its own id and this one is discarded.
pub async fn sync_vault(
    db:        &kubuno_db::DbPool,
    user_id:   Uuid,
    kdbx_path: &str,
    file_size: i64,
    hash:      &str,
) -> Result<i64> {
    let insert = format!(
        "INSERT INTO keestore.vaults
           (id, owner_id, kdbx_path, file_size_bytes, sync_version,
            file_hash_sha256, last_modified_at)
           VALUES ($1, $2, $3, $4, 1, $5, $6){}",
        kubuno_db::dialect::upsert(
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
        "sync_version",
        |q| {
            q.bind(kubuno_db::new_id())
                .bind(user_id)
                .bind(kdbx_path)
                .bind(file_size)
                .bind(hash)
                .bind(now)
        },
        "SELECT sync_version FROM keestore.vaults WHERE owner_id = $1",
        |q| q.bind(user_id),
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
    match kubuno_db::query(
        "UPDATE keestore.vaults SET last_accessed_at = $1 WHERE owner_id = $2",
    ) {
        Ok(q) => {
            if let Err(e) = q.bind(Utc::now()).bind(user_id).execute(db).await {
                tracing::warn!(error = %e, "Horodatage de dernier accès non enregistré");
            }
        }
        Err(e) => tracing::error!(error = %e, "Requête d'horodatage invalide"),
    }
}

/// Removes the vault row. The blob itself is deleted by the caller.
pub async fn delete_vault(db: &kubuno_db::DbPool, user_id: Uuid) -> Result<()> {
    kubuno_db::query("DELETE FROM keestore.vaults WHERE owner_id = $1")?
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}
