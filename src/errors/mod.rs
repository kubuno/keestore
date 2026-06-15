use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum KeeStoreError {
    #[error("Non authentifié")]
    Unauthorized,

    #[error("Accès refusé")]
    Forbidden,

    #[error("Coffre introuvable")]
    VaultNotFound,

    #[error("Fichier invalide: {0}")]
    InvalidFile(String),

    #[error("Fichier trop volumineux")]
    FileTooLarge,

    #[error("Conflit de synchronisation (version serveur: {server_version}, version client: {client_version})")]
    SyncConflict { server_version: i64, client_version: i64 },

    #[error("Requête invalide: {0}")]
    BadRequest(String),

    #[error("Service HIBP indisponible")]
    HibpUnavailable,

    #[error("Erreur de stockage: {0}")]
    Storage(#[from] kubuno_storage::StorageError),

    #[error("Erreur base de données")]
    Database(#[from] sqlx::Error),

    #[error("Erreur interne")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for KeeStoreError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            KeeStoreError::Unauthorized     => (StatusCode::UNAUTHORIZED,         "UNAUTHORIZED",      self.to_string()),
            KeeStoreError::Forbidden        => (StatusCode::FORBIDDEN,            "FORBIDDEN",         self.to_string()),
            KeeStoreError::VaultNotFound    => (StatusCode::NOT_FOUND,            "VAULT_NOT_FOUND",   self.to_string()),
            KeeStoreError::InvalidFile(_)   => (StatusCode::BAD_REQUEST,          "INVALID_FILE",      self.to_string()),
            KeeStoreError::FileTooLarge     => (StatusCode::PAYLOAD_TOO_LARGE,    "FILE_TOO_LARGE",    self.to_string()),
            KeeStoreError::SyncConflict { .. } => (StatusCode::CONFLICT,          "SYNC_CONFLICT",     self.to_string()),
            KeeStoreError::BadRequest(_)    => (StatusCode::BAD_REQUEST,          "BAD_REQUEST",       self.to_string()),
            KeeStoreError::HibpUnavailable  => (StatusCode::BAD_GATEWAY,          "HIBP_UNAVAILABLE",  self.to_string()),
            KeeStoreError::Storage(e) => {
                tracing::error!(error = %e, "Storage error");
                (StatusCode::INTERNAL_SERVER_ERROR, "STORAGE_ERROR", "Erreur de stockage".to_string())
            }
            KeeStoreError::Database(e) => {
                tracing::error!(error = %e, "Database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR", "Erreur base de données".to_string())
            }
            KeeStoreError::Internal(e) => {
                tracing::error!(error = %e, "Internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", "Erreur interne".to_string())
            }
        };

        (status, Json(json!({ "error": code, "message": message }))).into_response()
    }
}

pub type Result<T> = std::result::Result<T, KeeStoreError>;
