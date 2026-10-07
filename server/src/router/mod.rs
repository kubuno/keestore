use axum::{
    middleware,
    routing::get,
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    handlers::{health, hibp, kdbx},
    middleware::require_auth,
    state::AppState,
};

pub fn build(state: AppState) -> Router {
    let authed = Router::new()
        .route("/kdbx",   get(kdbx::get_kdbx)
                              .put(kdbx::put_kdbx)
                              .delete(kdbx::delete_kdbx))
        .route("/status",       get(kdbx::get_status))
        .route("/hibp/:prefix", get(hibp::hibp_proxy))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .with_state(state.clone());

    let system = Router::new()
        .route("/health", get(health::health))
        .with_state(state);

    Router::new()
        .merge(system)
        .nest("/", authed)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
