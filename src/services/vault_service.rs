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
    db:      &sqlx::PgPool,
) -> Result<VaultMeta> {
    let row = sqlx::query(
        r#"SELECT id, owner_id, kdbx_path, file_size_bytes, sync_version,
                  file_hash_sha256, last_accessed_at, last_modified_at,
                  unlock_attempts, locked_until, created_at, updated_at
           FROM keestore.vaults
           WHERE owner_id = $1"#,
    )
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
