use anyhow::Result;
use arnold_wire::DaemonEvent;
use serde_json::{json, Value};
use crate::handler::HandlerContext;
use crate::notify;

pub async fn send_notification(
    ctx: &HandlerContext,
    title: String,
    body: String,
    urgency: Option<String>,
) -> Result<Value> {
    let urgency_str = urgency.clone().unwrap_or_else(|| "normal".to_string());

    // Best-effort OS dispatch — if the platform CLI is missing we still emit
    // the in-app event so the TUI shows it.
    let os_result = notify::dispatch(&title, &body, urgency.as_deref()).await;

    // Always emit the wire event regardless of OS dispatch success.
    let _ = ctx.session.client.send(DaemonEvent::Notification {
        session_id: ctx.session.session_id,
        title: title.clone(),
        body: body.clone(),
        urgency: urgency_str.clone(),
    });

    match os_result {
        Ok(()) => Ok(json!({ "status": "dispatched" })),
        Err(e) => Ok(json!({
            "status": "tui_only",
            "note": format!("OS notification failed but TUI event emitted: {e}"),
        })),
    }
}
