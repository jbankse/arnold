use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpFrame {
    Syscall { id: u64, method: String, params: Value },
    Finished,
    Usage {
        #[serde(default)] provider: String,
        #[serde(default)] model: String,
        #[serde(default)] input_tokens: u64,
        #[serde(default)] output_tokens: u64,
        #[serde(default)] total_tokens: u64,
        #[serde(default)] estimated_cost_usd: Option<f64>,
        #[serde(default)] actual_cost_usd: Option<f64>,
    },
    /// Catch-all for any frame type the daemon doesn't directly handle
    /// (archive_l2, provider_error, etc.). The session loop pattern-matches
    /// the raw Value to surface provider_error as DaemonEvent::Error.
    #[serde(other)]
    Other,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownFrame<'a> {
    Plan(Value),
    Step { task_id: &'a str },
    Result { id: u64, result: Value },
    Error { id: u64, message: String },
}

pub struct CpuProcess {
    child: Child,
    stdin: ChildStdin,
    stdout_lines: tokio::io::Lines<BufReader<ChildStdout>>,
    task_id: String,
    /// Most-recent ~16k of cpu's stderr, drained continuously by a background
    /// task so we can surface it as a user-facing error when cpu fatals without
    /// emitting a `finished` up-frame (e.g. missing ANTHROPIC_API_KEY).
    stderr_tail: Arc<Mutex<String>>,
}

impl CpuProcess {
    pub async fn spawn(
        binary: &Path,
        provider: &str,
        model: &str,
        task_id: String,
    ) -> Result<Self> {
        // Note: provider API keys (ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.) flow via
        // parent-process env inheritance (tokio::process::Command does not env_clear by default).
        let mut child = Command::new(binary)
            .env("AGENT_LLM_PROVIDER", provider)
            .env("AGENT_MODEL", model)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| anyhow!("failed to spawn cpu at {}: {e}", binary.display()))?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin on cpu"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout on cpu"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr on cpu"))?;
        let stdout_lines = BufReader::new(stdout).lines();

        let stderr_tail = Arc::new(Mutex::new(String::new()));
        let tail_writer = stderr_tail.clone();
        let mut stderr_lines = BufReader::new(stderr).lines();
        tokio::spawn(async move {
            while let Ok(Some(line)) = stderr_lines.next_line().await {
                if let Ok(mut buf) = tail_writer.lock() {
                    buf.push_str(&line);
                    buf.push('\n');
                    if buf.len() > 16_000 {
                        let drain_until = buf.len() - 12_000;
                        buf.drain(..drain_until);
                    }
                }
            }
        });

        Ok(Self { child, stdin, stdout_lines, task_id, stderr_tail })
    }

    /// Snapshot the buffered cpu stderr so far. Used by the session loop to
    /// surface a meaningful error when cpu fatals before emitting any up-frame.
    pub fn stderr_tail(&self) -> String {
        self.stderr_tail.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub async fn send_plan(&mut self, plan: Value) -> Result<()> {
        self.write_frame(&DownFrame::Plan(plan)).await
    }

    pub async fn send_step(&mut self) -> Result<()> {
        let task_id = self.task_id.clone();
        let frame = DownFrame::Step { task_id: &task_id };
        self.write_frame(&frame).await
    }

    pub async fn send_result(&mut self, id: u64, result: Value) -> Result<()> {
        self.write_frame(&DownFrame::Result { id, result }).await
    }

    pub async fn send_error(&mut self, id: u64, message: String) -> Result<()> {
        self.write_frame(&DownFrame::Error { id, message }).await
    }

    pub async fn read_up(&mut self) -> Result<Option<(UpFrame, Value)>> {
        loop {
            let Some(line) = self.stdout_lines.next_line().await? else { return Ok(None); };
            if line.trim().is_empty() { continue; }
            let raw: Value = serde_json::from_str(&line)
                .map_err(|e| anyhow!("cpu sent malformed frame '{line}': {e}"))?;
            let frame: UpFrame = serde_json::from_value(raw.clone())
                .map_err(|e| anyhow!("cpu sent malformed frame '{line}': {e}"))?;
            return Ok(Some((frame, raw)));
        }
    }

    async fn write_frame<T: serde::Serialize>(&mut self, frame: &T) -> Result<()> {
        let mut bytes = serde_json::to_vec(frame)?;
        bytes.push(b'\n');
        self.stdin.write_all(&bytes).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    pub async fn shutdown(mut self) -> Result<()> {
        drop(self.stdin);
        let _ = self.child.kill().await;
        Ok(())
    }
}
