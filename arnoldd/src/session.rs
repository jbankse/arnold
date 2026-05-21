use anyhow::Result;
use arnold_wire::DaemonEvent;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

/// One sender per active client connection. Daemon pushes DaemonEvent through it.
pub type ClientSender = mpsc::UnboundedSender<DaemonEvent>;

#[derive(Clone)]
pub struct SessionState {
    pub session_id: Uuid,
    pub cwd: PathBuf,
    pub client: ClientSender,
}

#[derive(Default, Clone)]
pub struct SessionRegistry {
    inner: Arc<Mutex<HashMap<Uuid, SessionState>>>,
}

impl SessionRegistry {
    pub async fn open(&self, cwd: PathBuf, client: ClientSender) -> Uuid {
        let id = Uuid::new_v4();
        let state = SessionState { session_id: id, cwd, client };
        self.inner.lock().await.insert(id, state);
        id
    }

    pub async fn get(&self, id: Uuid) -> Option<SessionState> {
        self.inner.lock().await.get(&id).cloned()
    }

    pub async fn close(&self, id: Uuid) {
        self.inner.lock().await.remove(&id);
    }
}

/// Stub turn handler — Task 11+ replaces this with cpu integration.
pub async fn handle_user_message_stub(session: SessionState, text: String) -> Result<()> {
    let reply = format!("echo (stub): {text}");
    let _ = session.client.send(DaemonEvent::Reply { session_id: session.session_id, text: reply });
    let _ = session.client.send(DaemonEvent::TurnComplete { session_id: session.session_id });
    Ok(())
}
