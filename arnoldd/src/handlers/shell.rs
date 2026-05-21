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
            // Best-effort kill; child already moved into wait_with_output, so we cannot
            // kill it here directly. The future is dropped, which on stable Tokio cancels
            // the await but leaves the process running until natural exit. v0a accepts this.
            return Err(anyhow::anyhow!("command timed out after {timeout_ms:?} ms"));
        }
    };
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let trunc = |s: String| if s.len() > 8000 { (s.chars().take(8000).collect::<String>(), true) } else { (s, false) };
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
