use anyhow::Result;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use crate::handler::HandlerContext;

pub async fn run_command(
    ctx: &HandlerContext,
    cmd: String,
    args: Vec<String>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    stdin: Option<String>,
) -> Result<Value> {
    let cwd_abs = match cwd {
        Some(p) => ctx.jail.resolve(&p)?,
        None => ctx.session.cwd.clone(),
    };
    let mut child = Command::new(&cmd)
        .args(&args)
        .current_dir(&cwd_abs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    if let Some(input) = stdin {
        if let Some(mut s) = child.stdin.take() {
            s.write_all(input.as_bytes()).await?;
            drop(s);
        }
    }

    let timeout = Duration::from_millis(timeout_ms.unwrap_or(60_000));
    let out = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(o) => o?,
        Err(_) => {
            // The wait_with_output future owns the Child; dropping it on timeout
            // also drops the Child, which SIGKILLs the underlying process because
            // we set kill_on_drop(true) on the Command above.
            return Err(anyhow::anyhow!("command timed out after {timeout_ms:?} ms"));
        }
    };
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let trunc = |s: String| {
        if s.chars().count() > 8000 {
            (s.chars().take(8000).collect::<String>(), true)
        } else {
            (s, false)
        }
    };
    let (stdout_t, stdout_truncated) = trunc(stdout);
    let (stderr_t, stderr_truncated) = trunc(stderr);
    Ok(json!({
        "exit_code": out.status.code(),
        "stdout": stdout_t,
        "stderr": stderr_t,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
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
    async fn echo_returns_stdout_and_zero_exit() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        let r = run_command(
            &c,
            "echo".into(),
            vec!["hello".into()],
            None,
            None,
            None,
        ).await.unwrap();
        assert_eq!(r["exit_code"].as_i64(), Some(0));
        assert!(r["stdout"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn timeout_returns_err() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        // 100ms timeout on a `sleep 5` — must error with the timeout message.
        let r = run_command(
            &c,
            "sleep".into(),
            vec!["5".into()],
            None,
            Some(100),
            None,
        ).await;
        assert!(r.is_err(), "expected timeout error, got: {r:?}");
        let err_str = r.unwrap_err().to_string();
        assert!(err_str.contains("timed out"), "unexpected error: {err_str}");
    }
}
