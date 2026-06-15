use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VaultMeta {
    pub id:               Uuid,
    pub owner_id:         Uuid,
    pub kdbx_path:        String,
    pub file_size_bytes:  i64,
    pub sync_version:     i64,
    pub file_hash_sha256: Option<String>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub last_modified_at: Option<DateTime<Utc>>,
    pub unlock_attempts:  i32,
    pub locked_until:     Option<DateTime<Utc>>,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct VaultStatus {
    pub exists:           bool,
    pub sync_version:     i64,
    pub file_size_bytes:  i64,
    pub last_modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct VaultSyncResponse {
    pub sync_version: i64,
    pub file_hash:    String,
    pub message:      String,
}

#[derive(Debug, Deserialize)]
pub struct SyncVersionHeader(pub i64);
