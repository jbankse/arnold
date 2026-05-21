use anyhow::Result;
use serde_json::{json, Value};
use std::time::Duration;
use crate::handler::HandlerContext;

pub async fn web_fetch(_ctx: &HandlerContext, url: String) -> Result<Value> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow::anyhow!("only http(s) URLs are allowed"));
    }
    let redirect_policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 3 {
            return attempt.error("too many redirects (limit 3)");
        }
        let scheme = attempt.url().scheme().to_owned();
        if scheme != "http" && scheme != "https" {
            return attempt.error(format!("redirect to non-http(s) scheme '{scheme}' rejected"));
        }
        attempt.follow()
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Arnold/0.1 (+https://github.com/jbankse/Arnold)")
        .redirect(redirect_policy)
        .build()?;
    let resp = client.get(&url).send().await?;
    let status = resp.status().as_u16();
    let body = resp.text().await?;
    let truncated = body.chars().take(16_000).collect::<String>();
    Ok(json!({
        "status": status,
        "body": truncated,
        "body_truncated": body.chars().count() > 16_000,
    }))
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
        let client = tx.clone();
        HandlerContext {
            jail: Arc::new(Jail::new(vec![root.to_path_buf()])),
            memory: Arc::new(MemoryStore::open(&root.join("mem")).unwrap()),
            jobs,
            bios_binary: root.join("bios"),
            runtime_image: "runtime/os:local".to_string(),
            arnold_dir: root.to_path_buf(),
            session: SessionState { session_id: Uuid::nil(), cwd: root.to_path_buf(), client: tx },
            client,
        }
    }

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        assert!(web_fetch(&c, "file:///etc/passwd".into()).await.is_err());
    }

    #[tokio::test]
    async fn rejects_unknown_scheme() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        assert!(web_fetch(&c, "javascript:alert(1)".into()).await.is_err());
    }
}
