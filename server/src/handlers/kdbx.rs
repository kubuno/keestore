use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};

use crate::{
    errors::{KeeStoreError, Result},
    middleware::KeeStoreUser,
    models::{VaultStatus, VaultSyncResponse},
    services::vault_service::{
        delete_vault, get_vault_meta, is_kdbx_magic, kdbx_path, sha256_hex, sync_vault,
        touch_last_accessed,
    },
    state::AppState,
};

/// GET /kdbx — télécharger le fichier .kdbx
pub async fn get_kdbx(
    State(state): State<AppState>,
    Extension(user): Extension<KeeStoreUser>,
) -> Result<Response> {
    let vault = get_vault_meta(&user.id, &state.db).await?;

    touch_last_accessed(&state.db, user.id).await;

    let bytes = state.storage
        .get(&vault.kdbx_path)
        .await
        .map_err(|_| KeeStoreError::VaultNotFound)?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE,        "application/octet-stream".parse().unwrap());
    headers.insert(header::CONTENT_DISPOSITION, "attachment; filename=\"vault.kdbx\"".parse().unwrap());
    headers.insert(
        "x-sync-version".parse::<axum::http::HeaderName>().unwrap(),
        vault.sync_version.to_string().parse().unwrap(),
    );
    if let Some(hash) = &vault.file_hash_sha256 {
        headers.insert(
            "x-file-hash".parse::<axum::http::HeaderName>().unwrap(),
            hash.parse().unwrap_or_else(|_| "".parse().unwrap()),
        );
    }

    Ok((StatusCode::OK, headers, bytes).into_response())
}

/// PUT /kdbx — uploader le fichier .kdbx modifié
pub async fn put_kdbx(
    State(state): State<AppState>,
    Extension(user): Extension<KeeStoreUser>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<VaultSyncResponse>> {
    if body.is_empty() {
        return Err(KeeStoreError::InvalidFile("Fichier vide".to_string()));
    }
    if body.len() as u64 > state.instance().max_kdbx_size_bytes {
        return Err(KeeStoreError::FileTooLarge);
    }
    if !is_kdbx_magic(&body) {
        return Err(KeeStoreError::InvalidFile(
            "Signature KDBX invalide — le fichier n'est pas un .kdbx valide".to_string(),
        ));
    }

    // Détection de conflit via X-Sync-Version (optionnel)
    if let Some(client_version) = headers
        .get("x-sync-version")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
    {
        if let Ok(current) = get_vault_meta(&user.id, &state.db).await {
            if current.sync_version != client_version {
                return Err(KeeStoreError::SyncConflict {
                    server_version: current.sync_version,
                    client_version,
                });
            }
        }
    }

    let hash      = sha256_hex(&body);
    let path      = kdbx_path(user.id);
    let file_size = body.len() as i64;

    state.storage.put(&path, body).await?;

    let sync_version = sync_vault(&state.db, user.id, &path, file_size, &hash).await?;

    Ok(Json(VaultSyncResponse {
        sync_version,
        file_hash: hash,
        message:   "Coffre synchronisé".to_string(),
    }))
}

/// DELETE /kdbx — supprimer définitivement le coffre
pub async fn delete_kdbx(
    State(state): State<AppState>,
    Extension(user): Extension<KeeStoreUser>,
) -> Result<StatusCode> {
    let vault = get_vault_meta(&user.id, &state.db).await?;
    state.storage.delete(&vault.kdbx_path).await?;
    delete_vault(&state.db, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /status — vérifier l'existence du coffre sans le télécharger
pub async fn get_status(
    State(state): State<AppState>,
    Extension(user): Extension<KeeStoreUser>,
) -> Result<Json<VaultStatus>> {
    match get_vault_meta(&user.id, &state.db).await {
        Ok(vault) => Ok(Json(VaultStatus {
            exists:           true,
            sync_version:     vault.sync_version,
            file_size_bytes:  vault.file_size_bytes,
            last_modified_at: vault.last_modified_at,
            file_hash:        vault.file_hash_sha256,
        })),
        Err(KeeStoreError::VaultNotFound) => Ok(Json(VaultStatus {
            exists:           false,
            sync_version:     0,
            file_size_bytes:  0,
            last_modified_at: None,
            file_hash:        None,
        })),
        Err(e) => Err(e),
    }
}
