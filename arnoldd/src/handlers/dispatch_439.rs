use anyhow::Result;
use arnold_wire::DaemonEvent;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;
use crate::bios_process;
use crate::handler::HandlerContext;
use crate::jobs::JobStatus;

pub async fn spin_up_439(
    ctx: &HandlerContext,
    prompt: String,
    pack_kind: Option<String>,
    _export_workspace_to: Option<String>,
    _env: Option<HashMap<String, String>>,
) -> Result<Value> {
    // `_export_workspace_to` and `_env` are reserved for v0.1 — v0b ignores
    // them and uses the default ~/.arnold/jobs/<job_id>/workspace path.

    let job_id = ctx.jobs.create(ctx.session.session_id, &prompt, pack_kind.as_deref())?;

    // Immediately push JobSpawned to the client so the TUI's jobs pane
    // shows the new row before we even start bios.
    let preview: String = prompt.chars().take(80).collect();
    let _ = ctx.session.client.send(DaemonEvent::JobSpawned {
        session_id: ctx.session.session_id,
        job_id,
        prompt_preview: preview,
    });

    // Spawn the BIOS run in the background. The session's turn loop returns
    // immediately to the LLM with { job_id }. JobCompleted gets pushed via
    // the cloned client sender when bios exits.
    let table = ctx.jobs.clone();
    let bios_binary = ctx.bios_binary.clone();
    let runtime_image = ctx.runtime_image.clone();
    let arnold_dir = ctx.arnold_dir.clone();
    let client = ctx.session.client.clone();
    let session_id = ctx.session.session_id;
    let pack_kind_clone = pack_kind.clone();

    tokio::spawn(async move {
        let outcome = bios_process::spawn_and_drive(
            &bios_binary,
            &runtime_image,
            &arnold_dir,
            &table,
            job_id,
            &prompt,
            pack_kind_clone.as_deref(),
        ).await;

        match outcome {
            Ok(o) => {
                let _ = client.send(DaemonEvent::JobCompleted {
                    session_id,
                    job_id,
                    status: o.status.as_str().to_string(),
                    exit_code: o.exit_code,
                    exported_workspace: o.exported_path,
                    message: o.message,
                });
            }
            Err(e) => {
                let msg = format!("bios driver error: {e:#}");
                let _ = table.finalize(job_id, JobStatus::Failed, None, None, &msg);
                let _ = client.send(DaemonEvent::JobCompleted {
                    session_id,
                    job_id,
                    status: "failed".into(),
                    exit_code: None,
                    exported_workspace: None,
                    message: msg,
                });
            }
        }
    });

    Ok(json!({ "job_id": job_id.to_string(), "status": "queued" }))
}

pub async fn poll_job(ctx: &HandlerContext, job_id: String) -> Result<Value> {
    let jid = Uuid::parse_str(&job_id)
        .map_err(|_| anyhow::anyhow!("invalid job_id '{job_id}'"))?;
    match ctx.jobs.get(jid)? {
        Some(job) => Ok(json!({
            "job_id": job.job_id.to_string(),
            "status": job.status.as_str(),
            "exit_code": job.exit_code,
            "exported_path": job.exported_path,
            "last_log_line": job.last_log_line,
            "message": job.message,
            "started_at": job.started_at.to_rfc3339(),
            "updated_at": job.updated_at.to_rfc3339(),
            "completed_at": job.completed_at.map(|t| t.to_rfc3339()),
        })),
        None => Err(anyhow::anyhow!("unknown job_id '{job_id}'")),
    }
}
