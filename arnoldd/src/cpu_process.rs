use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
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
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| anyhow!("failed to spawn cpu at {}: {e}", binary.display()))?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin on cpu"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout on cpu"))?;
        let stdout_lines = BufReader::new(stdout).lines();
        Ok(Self { child, stdin, stdout_lines, task_id })
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
