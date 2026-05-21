use anyhow::{anyhow, Result};
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;
use crate::config::ArnoldConfig;
use crate::cpu_process::{CpuProcess, UpFrame};
use crate::plan_frame::PlanFrameBuilder;

/// Pre-flight a cpu spawn. Spawns the binary, sends a minimal plan frame, then
/// expects cpu to either emit a syscall (which we error-back so cpu exits) or
/// finished. Returns Ok if cpu came up and shut down cleanly.
pub async fn run(config: &ArnoldConfig) -> Result<String> {
    let task_id = Uuid::new_v4();
    let mut cpu = CpuProcess::spawn(
        &config.cpu_binary,
        &config.llm_provider,
        &config.model,
        task_id.to_string(),
    ).await.map_err(|e| anyhow!("failed to spawn cpu: {e}"))?;

    let context = "PROJECT_ROOT: /tmp\nPATH_MODE: project-relative\n\nUSER:\nprecheck\n".to_string();
    let plan = PlanFrameBuilder {
        task_id,
        context,
        model_provider: config.llm_provider.clone(),
        model: config.model.clone(),
    }.build()?;

    cpu.send_plan(plan).await?;
    cpu.send_step().await?;

    // Default 30s budget; cold-start Anthropic first-turn latency can plausibly cross
    // 10s on slow networks. Override via `ARNOLDD_PRECHECK_TIMEOUT_SECS` if needed.
    let budget_secs: u64 = std::env::var("ARNOLDD_PRECHECK_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let budget = Duration::from_secs(budget_secs);
    let outcome = timeout(budget, async {
        loop {
            match cpu.read_up().await {
                Ok(None) => return Err(anyhow!("cpu closed stdout before emitting finished")),
                Ok(Some((UpFrame::Finished, _))) => return Ok("cpu finished".to_string()),
                Ok(Some((UpFrame::Syscall { id, .. }, _))) => {
                    let _ = cpu.send_error(id, "precheck mode: not executing syscalls".into()).await;
                    cpu.send_step().await?;
                }
                Ok(Some((UpFrame::Usage { .. }, _))) => continue,
                Ok(Some((UpFrame::Other, raw))) => {
                    // cpu emits provider_error frames for fatal LLM-call failures
                    // (bad auth, exhausted retries, refusal). Surface them so --check
                    // reports the real cause instead of a generic timeout.
                    if raw.get("type").and_then(|v| v.as_str()) == Some("provider_error") {
                        let message = raw.get("message")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("provider_error (no message): {raw}"));
                        return Err(anyhow!("provider error during precheck: {message}"));
                    }
                    continue;
                }
                Err(e) => return Err(anyhow!("cpu read error: {e}")),
            }
        }
    }).await;

    let result = match outcome {
        Ok(r) => r,
        Err(_) => Err(anyhow!("cpu precheck timed out after {budget_secs}s")),
    };

    let _ = cpu.shutdown().await;
    result
}
