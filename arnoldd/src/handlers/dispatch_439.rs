use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use crate::handler::HandlerContext;

pub async fn spin_up_439(
    _ctx: &HandlerContext,
    _prompt: String,
    _pack_kind: Option<String>,
    _export_workspace_to: Option<String>,
    _env: Option<HashMap<String, String>>,
) -> Result<Value> { unimplemented!("Task 8") }

pub async fn poll_job(_ctx: &HandlerContext, _job_id: String) -> Result<Value> { unimplemented!("Task 8") }
