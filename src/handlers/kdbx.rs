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
    services::vault_service::{get_vault_meta, is_kdbx_magic, kdbx_path, sha256_hex},
    state::AppState,
};

/// GET /kdbx — télécharger le fichier .kdbx
pub async fn get_kdbx(
    State(state): State<AppState>,
    Extension(user): Extension<KeeStoreUser>,
) -> Result<Response> {
    let vault = get_vault_meta(&user.id, &state.db).await?;

    let _ = sqlx::query(
        "UPDATE keestore.vaults SET last_accessed_at = NOW() WHERE owner_id = $1",
    )
    .bind(user.id)
    .execute(&state.db)
    .await;

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
    if body.len() as u64 > state.settings.keestore.max_kdbx_size_bytes {
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

    use sqlx::Row;
    let row = sqlx::query(
        r#"INSERT INTO keestore.vaults
           (owner_id, kdbx_path, file_size_bytes, sync_version,
            file_hash_sha256, last_modified_at)
           VALUES ($1, $2, $3, 1, $4, NOW())
           ON CONFLICT (owner_id) DO UPDATE SET
               kdbx_path        = EXCLUDED.kdbx_path,
               file_size_bytes  = EXCLUDED.file_size_bytes,
               sync_version     = keestore.vaults.sync_version + 1,
               file_hash_sha256 = EXCLUDED.file_hash_sha256,
               last_modified_at = NOW()
           RETURNING sync_version"#,
    )
    .bind(user.id)
    .bind(&path)
    .bind(file_size)
    .bind(&hash)
    .fetch_one(&state.db)
    .await?;

    let sync_version: i64 = row.try_get("sync_version")?;

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
    sqlx::query("DELETE FROM keestore.vaults WHERE owner_id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await?;
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
        })),
        Err(KeeStoreError::VaultNotFound) => Ok(Json(VaultStatus {
            exists:           false,
            sync_version:     0,
            file_size_bytes:  0,
            last_modified_at: None,
        })),
        Err(e) => Err(e),
    }
}
