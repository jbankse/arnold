use anyhow::Result;
use serde_json::Value;
use crate::handler::HandlerContext;

pub async fn reply(_ctx: &HandlerContext, _text: String) -> Result<Value> { unimplemented!("Task 13") }
pub async fn done(_ctx: &HandlerContext) -> Result<Value> { unimplemented!("Task 13") }
