use anyhow::Result;
use arnold_wire::DaemonEvent;
use serde_json::{json, Value};
use crate::handler::HandlerContext;

pub async fn reply(ctx: &HandlerContext, text: String) -> Result<Value> {
    let _ = ctx.session.client.send(DaemonEvent::Reply {
        session_id: ctx.session.session_id,
        text: text.clone(),
    });
    Ok(json!({"status": "delivered"}))
}

pub async fn done(ctx: &HandlerContext) -> Result<Value> {
    let _ = ctx.session.client.send(DaemonEvent::TurnComplete {
        session_id: ctx.session.session_id,
    });
    Ok(json!({"status": "turn_complete"}))
}
