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
    pub sessions: Arc<Mutex<HashMap<Uuid, SessionState>>>,
}

impl DaemonState {
    pub fn new(config: ArnoldConfig, jail: Jail, memory: MemoryStore) -> Self {
        Self {
            config: Arc::new(config),
            jail: Arc::new(jail),
            memory: Arc::new(memory),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn open(&self, cwd: PathBuf, client: ClientSender) -> Uuid {
        let id = Uuid::new_v4();
        self.sessions.lock().await.insert(id, SessionState { session_id: id, cwd, client });
        id
    }

    pub async fn get(&self, id: Uuid) -> Option<SessionState> {
        self.sessions.lock().await.get(&id).cloned()
    }

    pub async fn close(&self, id: Uuid) {
        self.sessions.lock().await.remove(&id);
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

    let ctx = HandlerContext {
        jail: state.jail.clone(),
        memory: state.memory.clone(),
        session: session.clone(),
    };

    loop {
        let frame_with_raw = cpu.read_up().await?;
        match frame_with_raw {
            None => break,
            Some((UpFrame::Finished, _)) => break,
            Some((UpFrame::Usage { .. }, _)) => { continue; }
            Some((UpFrame::Other, raw)) => {
                // cpu emits provider_error frames when an LLM call fails terminally
                // (bad auth, exhausted retry budget, refusal, etc.). v0a surfaces
                // these to the user; everything else (usage, archive_l2) is ignored.
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
