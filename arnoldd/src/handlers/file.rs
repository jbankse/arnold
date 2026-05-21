use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use crate::handler::HandlerContext;

pub async fn read_file(ctx: &HandlerContext, path: String) -> Result<Value> {
    let abs = ctx.jail.resolve(&path)?;
    let contents = tokio::fs::read_to_string(&abs).await?;
    // Compare on the same unit (chars) to avoid byte/char mismatch on multi-byte UTF-8.
    let was_truncated = contents.chars().count() > 8000;
    let truncated = contents.chars().take(8000).collect::<String>();
    Ok(json!({"contents": truncated, "truncated": was_truncated}))
}

pub async fn write_file(ctx: &HandlerContext, path: String, contents: String) -> Result<Value> {
    let abs = ctx.jail.resolve(&path)?;
    if let Some(parent) = abs.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&abs, &contents).await?;
    Ok(json!({"bytes_written": contents.len()}))
}

pub async fn replace_in_file(
    ctx: &HandlerContext,
    path: String,
    old_string: String,
    new_string: String,
) -> Result<Value> {
    let abs = ctx.jail.resolve(&path)?;
    let original = tokio::fs::read_to_string(&abs).await?;
    let occurrences = original.matches(&old_string).count();
    if occurrences == 0 {
        return Err(anyhow!("old_string not found in file"));
    }
    if occurrences > 1 {
        return Err(anyhow!("old_string matched {occurrences} times; must be unique"));
    }
    let updated = original.replacen(&old_string, &new_string, 1);
    tokio::fs::write(&abs, &updated).await?;
    Ok(json!({"replaced": 1}))
}

pub async fn list_dir(ctx: &HandlerContext, path: String) -> Result<Value> {
    let abs = ctx.jail.resolve(&path)?;
    let mut entries = vec![];
    let mut rd = tokio::fs::read_dir(&abs).await?;
    let mut total_seen: usize = 0;
    let mut truncated = false;
    while let Some(e) = rd.next_entry().await? {
        total_seen += 1;
        if entries.len() >= 200 {
            truncated = true;
            // Drain the rest so the iterator's resource is freed cleanly.
            continue;
        }
        let ft = e.file_type().await?;
        entries.push(json!({
            "name": e.file_name().to_string_lossy(),
            "is_dir": ft.is_dir(),
        }));
    }
    Ok(json!({"entries": entries, "truncated": truncated, "total": total_seen}))
}

pub async fn search(
    ctx: &HandlerContext,
    query: String,
    path: Option<String>,
    glob: Option<String>,
) -> Result<Value> {
    let search_path = match path {
        Some(p) => ctx.jail.resolve(&p)?,
        None => ctx.session.cwd.clone(),
    };
    let mut cmd = Command::new("rg");
    cmd.arg("--json").arg("--max-count").arg("20").arg("--max-columns").arg("200");
    if let Some(g) = glob { cmd.arg("--glob").arg(g); }
    cmd.arg(&query).arg(&search_path);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);

    let mut child = cmd.spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => anyhow!(
                "ripgrep (rg) not found on PATH. Install via `brew install ripgrep` (macOS) or your distro's package manager (Linux)."
            ),
            _ => anyhow!("failed to spawn rg: {e}"),
        })?;
    let mut out = String::new();
    if let Some(mut so) = child.stdout.take() {
        so.read_to_string(&mut out).await?;
    }
    let _ = child.wait().await?;
    // Parse one line per match
    let mut matches = vec![];
    for line in out.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        if v["type"] == "match" {
            matches.push(json!({
                "path": v["data"]["path"]["text"],
                "line_number": v["data"]["line_number"],
                "text": v["data"]["lines"]["text"],
            }));
        }
        if matches.len() >= 50 { break; }
    }
    Ok(json!({"matches": matches}))
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

    fn ctx(jail_root: &std::path::Path) -> HandlerContext {
        let (tx, _rx) = mpsc::unbounded_channel();
        let conn = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let jobs = JobTable::attach(conn).unwrap();
        let client = tx.clone();
        HandlerContext {
            jail: Arc::new(Jail::new(vec![jail_root.to_path_buf()])),
            memory: Arc::new(MemoryStore::open(&jail_root.join("mem")).unwrap()),
            jobs,
            bios_binary: jail_root.join("bios"),
            runtime_image: "runtime/os:local".to_string(),
            arnold_dir: jail_root.to_path_buf(),
            session: SessionState {
                session_id: Uuid::nil(),
                cwd: jail_root.to_path_buf(),
                client: tx,
            },
            client,
        }
    }

    #[tokio::test]
    async fn write_then_read() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        write_file(&c, "hello.txt".into(), "world".into()).await.unwrap();
        let r = read_file(&c, "hello.txt".into()).await.unwrap();
        assert_eq!(r["contents"], "world");
    }

    #[tokio::test]
    async fn replace_requires_unique_match() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        write_file(&c, "a.txt".into(), "foo bar foo".into()).await.unwrap();
        let err = replace_in_file(&c, "a.txt".into(), "foo".into(), "baz".into()).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn list_dir_caps_entries_at_200() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        // Create 250 files in a sub-directory so the cap fires cleanly without
        // the `mem/` dir created by MemoryStore::open polluting the count.
        for i in 0..250 {
            write_file(&c, format!("big/f{i}.txt"), "x".into()).await.unwrap();
        }
        let r = list_dir(&c, "big".into()).await.unwrap();
        let entries = r["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 200, "list_dir should cap entries at 200");
        assert_eq!(r["truncated"].as_bool(), Some(true));
        assert_eq!(r["total"].as_u64(), Some(250));
    }
}
