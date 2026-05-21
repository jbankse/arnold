use anyhow::Result;
use serde_json::Value;
use crate::handler::HandlerContext;

pub async fn send_notification(
    _ctx: &HandlerContext,
    _title: String,
    _body: String,
    _urgency: Option<String>,
) -> Result<Value> { unimplemented!("Task 9") }
