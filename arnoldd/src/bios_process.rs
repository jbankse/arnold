use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;
use crate::jobs::{JobStatus, JobTable};

/// Spawn `bios run ...` and drive it to completion. Updates the job table as
/// status transitions happen; returns the terminal (status, exit_code,
/// exported_path, message) tuple for the caller to ship as JobCompleted.
///
/// Arguments:
/// - `bios_binary`: absolute path to the bios executable
/// - `runtime_image`: container image reference (e.g. "runtime/os:local")
/// - `arnold_dir`: Arnold's home (~/.arnold/) — used to derive the export path
/// - `job_id`: UUID of the job this spawn corresponds to
/// - `prompt`: the user-supplied task prompt
/// - `pack_kind`: optional 439 pack hint; currently ignored (bios autodetects)
pub async fn spawn_and_drive(
    bios_binary: &Path,
    runtime_image: &str,
    arnold_dir: &Path,
    table: &JobTable,
    job_id: Uuid,
    prompt: &str,
    pack_kind: Option<&str>,
) -> Result<JobOutcome> {
    let _ = pack_kind; // reserved for future use

    let job_dir = arnold_dir.join("jobs").join(job_id.to_string());
    let export_path = job_dir.join("workspace");
    std::fs::create_dir_all(&job_dir)?;

    // Move from queued to running BEFORE we spawn so the TUI shows the
    // transition immediately even if bios takes a few seconds to start.
    table.update_status(job_id, JobStatus::Running, Some("spawning bios"))?;

    let mut child = Command::new(bios_binary)
        .args([
            "run",
            "--provider", "docker",
            "--image", runtime_image,
            "--no-build",
            "--export-workspace", export_path.to_str()
                .ok_or_else(|| anyhow!("export path is not valid UTF-8: {}", export_path.display()))?,
            "--", prompt,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow!("failed to spawn bios at {}: {e}", bios_binary.display()))?;

    // Tail stdout for heartbeat updates to the job table's last_log_line.
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout on bios"))?;
    let mut stdout_lines = BufReader::new(stdout).lines();
    let table_clone = table.clone();
    let job_id_clone = job_id;
    let log_task = tokio::spawn(async move {
        while let Ok(Some(line)) = stdout_lines.next_line().await {
            // Update last_log_line. Errors are non-fatal — heartbeats are best-effort.
            let _ = table_clone.update_status(
                job_id_clone,
                JobStatus::Running,
                Some(line.chars().take(200).collect::<String>().as_str()),
            );
        }
    });

    // Drain stderr concurrently into a buffer so we can show it on failure.
    let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr on bios"))?;
    let mut stderr_lines = BufReader::new(stderr).lines();
    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        while let Ok(Some(line)) = stderr_lines.next_line().await {
            buf.push_str(&line);
            buf.push('\n');
            if buf.len() > 16_000 {
                // Truncate from the head so we keep the most recent lines.
                let drain_until = buf.len() - 12_000;
                buf.drain(..drain_until);
            }
        }
        buf
    });

    let status = child.wait().await.map_err(|e| anyhow!("waiting on bios: {e}"))?;
    let _ = log_task.await;
    let stderr_tail = stderr_task.await.unwrap_or_default();

    let exit_code = status.code();
    let succeeded = status.success();
    let final_status = if succeeded { JobStatus::Completed } else { JobStatus::Failed };

    // Transition to "exporting" briefly so the TUI shows the export step,
    // then to the terminal status. (Real bios export happens during its own
    // run; this is purely a UX heartbeat for v0b.)
    if succeeded {
        let _ = table.update_status(job_id, JobStatus::Exporting, Some("bios exported workspace"));
    }

    let exported_path_str = if succeeded {
        export_path.to_str().map(String::from)
    } else {
        None
    };

    let message = if succeeded {
        "bios completed; workspace exported".to_string()
    } else {
        format!("bios failed (exit {:?})\nstderr tail:\n{}", exit_code, stderr_tail.trim_end())
    };

    table.finalize(job_id, final_status.clone(), exit_code, exported_path_str.as_deref(), &message)?;

    Ok(JobOutcome {
        job_id,
        status: final_status,
        exit_code,
        exported_path: exported_path_str,
        message,
    })
}

pub struct JobOutcome {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub exit_code: Option<i32>,
    pub exported_path: Option<String>,
    pub message: String,
}

/// Locate the bios binary. Looks at config.bios_binary first, then
/// ~/.arnold/bin/bios, then `which bios`.
pub fn resolve_bios_binary(config_bios_path: Option<&Path>, arnold_dir: &Path) -> Result<PathBuf> {
    if let Some(p) = config_bios_path {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }
    let candidate = arnold_dir.join("bin").join("bios");
    if candidate.exists() {
        return Ok(candidate);
    }
    which::which("bios").map_err(|e| anyhow!("bios binary not found: {e}"))
}
