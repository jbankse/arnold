use anyhow::Result;
use serde_json::Value;
use crate::handler::HandlerContext;

pub async fn read_file(_ctx: &HandlerContext, _path: String) -> Result<Value> { unimplemented!("Task 13") }
pub async fn write_file(_ctx: &HandlerContext, _path: String, _contents: String) -> Result<Value> { unimplemented!("Task 13") }
pub async fn replace_in_file(_ctx: &HandlerContext, _path: String, _old_string: String, _new_string: String) -> Result<Value> { unimplemented!("Task 13") }
pub async fn list_dir(_ctx: &HandlerContext, _path: String) -> Result<Value> { unimplemented!("Task 13") }
pub async fn search(_ctx: &HandlerContext, _query: String, _path: Option<String>, _glob: Option<String>) -> Result<Value> { unimplemented!("Task 13") }
