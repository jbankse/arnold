use anyhow::Result;
use serde_json::{json, Value};
use std::time::Duration;
use crate::handler::HandlerContext;

pub async fn web_fetch(_ctx: &HandlerContext, url: String) -> Result<Value> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow::anyhow!("only http(s) URLs are allowed"));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Arnold/0.1 (+https://github.com/jbankse/Arnold)")
        .build()?;
    let resp = client.get(&url).send().await?;
    let status = resp.status().as_u16();
    let body = resp.text().await?;
    let truncated = body.chars().take(16_000).collect::<String>();
    Ok(json!({
        "status": status,
        "body": truncated,
        "body_truncated": body.len() > 16_000,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jail::Jail;
    use crate::memory::MemoryStore;
    use crate::session::SessionState;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    fn ctx(root: &std::path::Path) -> HandlerContext {
        let (tx, _rx) = mpsc::unbounded_channel();
        HandlerContext {
            jail: Arc::new(Jail::new(vec![root.to_path_buf()])),
            memory: Arc::new(MemoryStore::open(&root.join("mem")).unwrap()),
            session: SessionState { session_id: Uuid::nil(), cwd: root.to_path_buf(), client: tx },
        }
    }

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        assert!(web_fetch(&c, "file:///etc/passwd".into()).await.is_err());
    }
}
