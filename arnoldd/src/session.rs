use anyhow::Result;
use arnold_wire::DaemonEvent;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;
use crate::config::ArnoldConfig;
use crate::cpu_process::{CpuProcess, UpFrame};
use crate::handler::{HandlerContext, dispatch};
use crate::jail::Jail;
use crate::memory::MemoryStore;
use crate::plan_frame::PlanFrameBuilder;
use crate::syscall::Syscall;

pub type ClientSender = mpsc::UnboundedSender<DaemonEvent>;

#[derive(Clone)]
pub struct SessionState {
    pub session_id: Uuid,
    pub cwd: PathBuf,
    pub client: ClientSender,
}

#[derive(Clone)]
pub struct DaemonState {
    pub config: Arc<ArnoldConfig>,
    pub jail: Arc<Jail>,
    pub memory: Arc<MemoryStore>,
    pub jobs: crate::jobs::JobTable,
    pub arnold_dir: std::path::PathBuf,
    pub sessions: Arc<Mutex<HashMap<Uuid, SessionState>>>,
    pub usage: crate::usage_meter::UsageMeter,
}

impl DaemonState {
    pub fn new(
        config: ArnoldConfig,
        jail: Jail,
        memory: MemoryStore,
        jobs: crate::jobs::JobTable,
        arnold_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            config: Arc::new(config),
            jail: Arc::new(jail),
            memory: Arc::new(memory),
            jobs,
            arnold_dir,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            usage: crate::usage_meter::UsageMeter::default(),
        }
    }

    pub async fn open(&self, cwd: PathBuf, client: ClientSender) -> Uuid {
        let id = Uuid::new_v4();
        tracing::info!(session = %id, cwd = %cwd.display(), "session opened; cwd auto-allowlisted for this session");
        self.sessions.lock().await.insert(id, SessionState { session_id: id, cwd, client });
        id
    }

    pub async fn get(&self, id: Uuid) -> Option<SessionState> {
        self.sessions.lock().await.get(&id).cloned()
    }

    pub async fn close(&self, id: Uuid) {
        self.sessions.lock().await.remove(&id);
        let _ = self.usage.forget(id);
    }
}

/// Drive one user-message turn end-to-end.
pub async fn handle_user_message(state: DaemonState, session: SessionState, text: String) -> Result<()> {
    let task_id = Uuid::new_v4();
    let mut cpu = CpuProcess::spawn(
        &state.config.cpu_binary,
        &state.config.llm_provider,
        &state.config.model,
        task_id.to_string(),
    ).await?;

    let context = format!(
        "PROJECT_ROOT: {}\nPATH_MODE: project-relative\n\nUSER:\n{text}\n",
        session.cwd.display(),
    );
    let plan = PlanFrameBuilder {
        task_id,
        context,
        model_provider: state.config.llm_provider.clone(),
        model: state.config.model.clone(),
    }.build()?;

    cpu.send_plan(plan).await?;
    cpu.send_step().await?;

    // Per-session jail: take the daemon-wide roots + add this session's cwd. This is
    // what lets `arnold` Just Work from any shell directory — the TUI sends its cwd in
    // OpenSession and the daemon auto-allowlists it for the session's lifetime. The cwd
    // is trusted because it comes from the local UDS client (which is the user).
    let mut session_jail = (*state.jail).clone();
    session_jail.add_root(session.cwd.clone());
    let ctx = HandlerContext {
        jail: Arc::new(session_jail),
        memory: state.memory.clone(),
        jobs: state.jobs.clone(),
        bios_binary: state.config.bios_binary.clone(),
        runtime_image: state.config.runtime_image.clone(),
        arnold_dir: state.arnold_dir.clone(),
        session: session.clone(),
    };

    let mut saw_finished = false;
    loop {
        let frame_with_raw = cpu.read_up().await?;
        match frame_with_raw {
            None => {
                // cpu's stdout closed. If we never saw a `finished` frame, this
                // is a fatal (most commonly: missing ANTHROPIC_API_KEY, bad
                // binary, etc.). Surface the stderr tail so the TUI shows the
                // real cause instead of just hanging silently.
                if !saw_finished {
                    let tail = cpu.stderr_tail();
                    let trimmed = tail.trim();
                    let message = if trimmed.is_empty() {
                        "cpu exited without emitting any frame and produced no stderr; check ~/.arnold/arnoldd.err".to_string()
                    } else {
                        format!("cpu exited unexpectedly. stderr tail:\n{trimmed}")
                    };
                    let _ = session.client.send(DaemonEvent::Error {
                        session_id: Some(session.session_id),
                        message,
                    });
                }
                break;
            }
            Some((UpFrame::Finished, _)) => {
                saw_finished = true;
                break;
            }
            Some((UpFrame::Usage { provider, model, estimated_cost_usd, actual_cost_usd, .. }, _)) => {
                match state.usage.record(session.session_id, provider, model, estimated_cost_usd, actual_cost_usd) {
                    Ok(Some(tc)) => {
                        let _ = session.client.send(crate::usage_meter::cost_event(session.session_id, &tc));
                    }
                    Ok(None) => { /* no cost number; skip */ }
                    Err(e) => tracing::warn!("usage_meter.record failed: {e}"),
                }
                continue;
            }
            Some((UpFrame::Other, raw)) => {
                // cpu emits provider_error frames when an LLM call fails terminally
                // (bad auth, exhausted retry budget, refusal, etc.). v0a surfaces
                // these to the user; everything else (archive_l2, unknown frames) is ignored.
                if raw.get("type").and_then(|v| v.as_str()) == Some("provider_error") {
                    let message = raw.get("message")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("provider error (no message): {raw}"));
                    let _ = session.client.send(DaemonEvent::Error {
                        session_id: Some(session.session_id),
                        message: format!("provider error: {message}"),
                    });
                }
                continue;
            }
            Some((UpFrame::Syscall { id, method, params }, _)) => {
                let raw = serde_json::json!({ "method": method, "params": params });
                match serde_json::from_value::<Syscall>(raw) {
                    Ok(syscall) => match dispatch(&ctx, syscall).await {
                        Ok(result) => cpu.send_result(id, result).await?,
                        Err(e) => cpu.send_error(id, e.to_string()).await?,
                    },
                    Err(e) => cpu.send_error(id, format!("invalid syscall: {e}")).await?,
                }
                cpu.send_step().await?;
            }
        }
    }
    cpu.shutdown().await?;
    Ok(())
}
