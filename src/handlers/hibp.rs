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
    // Breach checking is an instance-level feature the admin can turn off. When
    // disabled, the route behaves as if it did not exist for this instance.
    let cfg = state.instance();
    if !cfg.enable_hibp {
        return Err(KeeStoreError::Forbidden);
    }

    let prefix = validate_prefix(&prefix)?;

    // Prefer the admin-set endpoint, falling back to config.toml when it was
    // never changed from the compiled default — an install that configured this
    // the old way keeps working until an admin edits it in the console.
    let d = crate::config::instance::InstanceConfig::default();
    let api_url = if cfg.hibp_api_url == d.hibp_api_url {
        state.settings.hibp.api_url.as_str()
    } else {
        cfg.hibp_api_url.as_str()
    };

    let url = format!("{api_url}/{prefix}");
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
