use axum::{extract::State, Json};
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Json<Value> {
    let db_ok = match kubuno_db::query("SELECT 1") {
        Ok(q) => q.execute(&state.db).await.is_ok(),
        Err(_) => false,
    };

    Json(json!({
        "status":  if db_ok { "ok" } else { "degraded" },
        "module":  "keestore",
        "version": env!("CARGO_PKG_VERSION"),
        "db":      if db_ok { "ok" } else { "error" },
    }))
}
