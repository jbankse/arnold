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

    // Read frames with a 10s budget; we tell cpu to fail on whatever it emits
    // first so it shuts down quickly. Any syscall we see -> send_error so cpu
    // moves on; finished -> success.
    let budget = Duration::from_secs(10);
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
                Ok(Some((UpFrame::Other, _))) => continue,
                Err(e) => return Err(anyhow!("cpu read error: {e}")),
            }
        }
    }).await;

    let result = match outcome {
        Ok(r) => r,
        Err(_) => Err(anyhow!("cpu precheck timed out after {budget:?}")),
    };

    let _ = cpu.shutdown().await;
    result
}
