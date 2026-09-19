use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};

/// Announces an event to the core, which fans it out to the modules that
/// subscribed to it.
///
/// This goes over the core's internal interface rather than a database
/// notification: `LISTEN/NOTIFY` exists only on PostgreSQL, and the core's
/// path is the sturdier one anyway — it queues the event, so nothing is lost
/// while a subscriber restarts.
pub async fn publish_event(
    client:          &Client,
    core_url:        &str,
    internal_secret: &str,
    event:           Value,
) -> Result<()> {
    let url = format!("{core_url}/internal/events/publish");
    client
        .post(&url)
        .header("X-Internal-Secret", internal_secret)
        .json(&event)
        .send()
        .await?;
    Ok(())
}

/// Best-effort announcement: a failure is logged and swallowed, because an
/// event that did not go out must never fail the operation that produced it.
pub async fn publish(
    client:          &Client,
    core_url:        &str,
    internal_secret: &str,
    event_type:      &str,
    payload:         Value,
) {
    let event = json!({ "type": event_type, "payload": payload });
    if let Err(e) = publish_event(client, core_url, internal_secret, event).await {
        tracing::warn!(error = %e, event_type, "Échec publication event");
    }
}
