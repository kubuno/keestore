use axum::{
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

use crate::{errors::KeeStoreError, state::AppState};

#[derive(Debug, Clone)]
pub struct KeeStoreUser {
    pub id:    Uuid,
    pub role:  String,
    pub email: String,
}

pub type KeeStoreUserExt = axum::Extension<KeeStoreUser>;

/// This module's id, the audience of the identity tokens the core mints for it.
const MODULE_ID: &str = "keestore";

/// Who the core says is calling, from the signed `X-Kubuno-Auth` token the core
/// mints with this module's internal secret (see `kubuno-modauth`).
///
/// The plain `X-Kubuno-User-*` headers are never read: any process reaching this
/// module's loopback port could set them and impersonate any user. A token is
/// only accepted when it was signed with this module's secret, for this module,
/// and has not expired. An **empty** configured secret refuses everything: an
/// HMAC keyed with nothing would let anyone mint a valid token.
pub fn authenticate(secret: &str, headers: &HeaderMap) -> Option<kubuno_modauth::ModuleUser> {
    if secret.is_empty() {
        tracing::error!(
            "keestore: core.internal_secret is empty, request refused. Set KUBUNO_INTERNAL_SECRET."
        );
        return None;
    }
    let token = headers.get(kubuno_modauth::TOKEN_HEADER)?.to_str().ok()?;
    kubuno_modauth::verify(secret.as_bytes(), token, MODULE_ID).ok()
}

/// Whether `provided` is the configured internal secret: never when that secret
/// is empty, and compared in constant time.
pub fn internal_secret_matches(expected: &str, provided: &str) -> bool {
    !expected.is_empty() && constant_time_eq(provided.as_bytes(), expected.as_bytes())
}

/// Byte comparison whose duration does not depend on where the first difference
/// is. The length check leaks the length, which is not a secret.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Authenticates the caller from the signed identity token (see [`authenticate`]).
/// This module has no internal or IPC route: every route but `/health` is a
/// user's, and goes through here.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> std::result::Result<Response, KeeStoreError> {
    let user = authenticate(&state.settings.core.internal_secret, req.headers())
        .ok_or(KeeStoreError::Unauthorized)?;
    req.extensions_mut()
        .insert(KeeStoreUser { id: user.id, role: user.role, email: user.email });
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    const SECRET: &str = "module-secret-for-tests";

    fn user() -> kubuno_modauth::ModuleUser {
        kubuno_modauth::ModuleUser {
            id: Uuid::new_v4(),
            role: "user".into(),
            email: "ada@example.org".into(),
        }
    }

    fn with_token(token: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(kubuno_modauth::TOKEN_HEADER, HeaderValue::from_str(token).expect("header"));
        h
    }

    #[test]
    fn forged_plain_headers_are_not_an_identity() {
        let mut h = HeaderMap::new();
        h.insert("x-kubuno-user-id", HeaderValue::from_static("0e835e1a-64fb-47c4-9775-5786236fce19"));
        h.insert("x-kubuno-user-role", HeaderValue::from_static("admin"));
        h.insert("x-kubuno-user-email", HeaderValue::from_static("admin@example.org"));
        assert!(authenticate(SECRET, &h).is_none());
    }

    #[test]
    fn a_token_signed_for_this_module_is_accepted() {
        let u = user();
        let token = kubuno_modauth::sign(SECRET.as_bytes(), &u, MODULE_ID);
        let got = authenticate(SECRET, &with_token(&token)).expect("valid token");
        assert_eq!(got.id, u.id);
        assert_eq!(got.email, u.email);
        assert_eq!(got.role, u.role);
    }

    #[test]
    fn a_token_for_another_module_or_key_is_refused() {
        let u = user();
        let other_aud = kubuno_modauth::sign(SECRET.as_bytes(), &u, "some-other-module");
        assert!(authenticate(SECRET, &with_token(&other_aud)).is_none());
        let other_key = kubuno_modauth::sign(b"another-secret", &u, MODULE_ID);
        assert!(authenticate(SECRET, &with_token(&other_key)).is_none());
        assert!(authenticate(SECRET, &with_token("v1.garbage.garbage")).is_none());
    }

    #[test]
    fn an_empty_secret_refuses_everything() {
        let token = kubuno_modauth::sign(b"", &user(), MODULE_ID);
        assert!(authenticate("", &with_token(&token)).is_none());
    }

    #[test]
    fn the_internal_secret_guard() {
        assert!(internal_secret_matches("abc", "abc"));
        assert!(!internal_secret_matches("abc", "abd"));
        assert!(!internal_secret_matches("abc", ""));
        assert!(!internal_secret_matches("", ""));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
