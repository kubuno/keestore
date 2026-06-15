use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension,
};

use crate::{
    errors::{KeeStoreError, Result},
    middleware::KeeStoreUser,
    services::hibp_service::validate_prefix,
    state::AppState,
};

/// GET /hibp/:prefix — proxy k-anonymity vers HaveIBeenPwned
pub async fn hibp_proxy(
    State(state): State<AppState>,
    Extension(_user): Extension<KeeStoreUser>,
    Path(prefix): Path<String>,
) -> Result<Response> {
    let prefix = validate_prefix(&prefix)?;

    let url = format!("{}/{}", state.settings.hibp.api_url, prefix);
    let resp = state.http
        .get(&url)
        .header("User-Agent", "KubunoKeestore/0.1")
        .header("Add-Padding", "true")
        .send()
        .await
        .map_err(|_| KeeStoreError::HibpUnavailable)?;

    let status = resp.status();
    let body   = resp.text().await.map_err(|_| KeeStoreError::HibpUnavailable)?;

    Ok((
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
        [(header::CONTENT_TYPE, "text/plain")],
        body,
    ).into_response())
}
