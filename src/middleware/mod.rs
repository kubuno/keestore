use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

use crate::{errors::KeeStoreError, state::AppState};

pub async fn require_ipc_secret(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> std::result::Result<Response, KeeStoreError> {
    let provided = req
        .headers()
        .get("x-internal-secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if provided != state.settings.core.internal_secret {
        return Err(KeeStoreError::Unauthorized);
    }
    Ok(next.run(req).await)
}

#[derive(Debug, Clone)]
pub struct KeeStoreUser {
    pub id:    Uuid,
    pub role:  String,
    pub email: String,
}

pub type KeeStoreUserExt = axum::Extension<KeeStoreUser>;

pub async fn require_auth(
    State(_state): State<AppState>,
    mut req: Request,
    next: Next,
) -> std::result::Result<Response, KeeStoreError> {
    let user_id = req
        .headers()
        .get("x-kubuno-user-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(KeeStoreError::Unauthorized)?;

    let role = req
        .headers()
        .get("x-kubuno-user-role")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("user")
        .to_string();

    let email = req
        .headers()
        .get("x-kubuno-user-email")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    req.extensions_mut().insert(KeeStoreUser { id: user_id, role, email });
    Ok(next.run(req).await)
}
