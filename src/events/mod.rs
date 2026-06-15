use serde_json::json;

/// Publie un événement vers le core via PG NOTIFY (best-effort).
pub async fn publish(
    db:         &sqlx::PgPool,
    event_type: &str,
    payload:    serde_json::Value,
) {
    let event = json!({ "type": event_type, "payload": payload });
    let payload_str = serde_json::to_string(&event).unwrap_or_default();
    if let Err(e) = sqlx::query("SELECT pg_notify('kubuno_events', $1)")
        .bind(&payload_str)
        .execute(db)
        .await
    {
        tracing::warn!(error = %e, event_type, "Échec publication event");
    }
}
