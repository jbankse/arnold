use anyhow::Result;
use serde_json::Value;
use crate::handler::HandlerContext;

pub async fn memory_read(_ctx: &HandlerContext, _topic: String) -> Result<Value> { unimplemented!("Task 15") }
pub async fn memory_write(_ctx: &HandlerContext, _topic: String, _content: String) -> Result<Value> { unimplemented!("Task 15") }
pub async fn memory_list(_ctx: &HandlerContext) -> Result<Value> { unimplemented!("Task 15") }
