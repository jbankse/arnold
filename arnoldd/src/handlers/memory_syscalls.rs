use anyhow::Result;
use serde_json::{json, Value};
use crate::handler::HandlerContext;
use crate::memory::MemoryTopic;

pub async fn memory_read(ctx: &HandlerContext, topic: String) -> Result<Value> {
    let t = ctx.memory.read(&topic)?;
    Ok(json!({
        "topic": topic,
        "name": t.name,
        "description": t.description,
        "type": serde_json::to_value(&t.kind)?,
        "body": t.body,
    }))
}

pub async fn memory_write(ctx: &HandlerContext, topic: String, content: String) -> Result<Value> {
    // The model passes the full file body (frontmatter + body). Parse it to validate.
    let parsed: MemoryTopic = parse_full_topic_file(&content)?;
    ctx.memory.write(&topic, &parsed)?;
    Ok(json!({"status": "written"}))
}

pub async fn memory_list(ctx: &HandlerContext) -> Result<Value> {
    Ok(json!({"index": ctx.memory.index()?}))
}

fn parse_full_topic_file(text: &str) -> Result<MemoryTopic> {
    let stripped = text.strip_prefix("---\n")
        .ok_or_else(|| anyhow::anyhow!("missing frontmatter; expected ---\\n"))?;
    let end = stripped.find("\n---\n")
        .ok_or_else(|| anyhow::anyhow!("unterminated frontmatter; expected closing \\n---\\n"))?;
    let frontmatter = &stripped[..end];
    let body = &stripped[end + 5..];
    #[derive(serde::Deserialize)]
    struct Fm { name: String, description: String, #[serde(rename = "type")] kind: crate::memory::MemoryType }
    let fm: Fm = serde_yaml::from_str(frontmatter)?;
    Ok(MemoryTopic { name: fm.name, description: fm.description, kind: fm.kind, body: body.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jail::Jail;
    use crate::jobs::JobTable;
    use crate::memory::MemoryStore;
    use crate::session::SessionState;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    fn ctx(root: &std::path::Path) -> HandlerContext {
        let (tx, _rx) = mpsc::unbounded_channel();
        let conn = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let jobs = JobTable::attach(conn).unwrap();
        HandlerContext {
            jail: Arc::new(Jail::new(vec![root.to_path_buf()])),
            memory: Arc::new(MemoryStore::open(&root.join("mem")).unwrap()),
            jobs,
            bios_binary: root.join("bios"),
            runtime_image: "runtime/os:local".to_string(),
            arnold_dir: root.to_path_buf(),
            session: SessionState { session_id: Uuid::nil(), cwd: root.to_path_buf(), client: tx },
        }
    }

    #[tokio::test]
    async fn write_then_read_then_list() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        let body = "---\nname: Test\ndescription: a test\ntype: user\n---\nbody text\n";
        memory_write(&c, "test".into(), body.into()).await.unwrap();
        let r = memory_read(&c, "test".into()).await.unwrap();
        assert_eq!(r["name"], "Test");
        let l = memory_list(&c).await.unwrap();
        assert!(l["index"].as_str().unwrap().contains("Test"));
    }
}
