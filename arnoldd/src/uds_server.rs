use std::path::{Path, PathBuf};
use anyhow::Result;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use arnold_wire::{ClientRequest, DaemonEvent};

use crate::session::{SessionRegistry, handle_user_message_stub};
use crate::inbox::{Inbox, InboxEvent};

pub async fn bind(path: &Path) -> Result<UnixListener> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub async fn handle_connection(
    stream: UnixStream,
    sessions: SessionRegistry,
    inbox: Inbox,
) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half).lines();
    let (tx, mut rx) = mpsc::unbounded_channel::<DaemonEvent>();

    // Write task: drain mpsc → write to socket
    let write_task = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let mut bytes = match serde_json::to_vec(&ev) { Ok(b) => b, Err(_) => continue };
            bytes.push(b'\n');
            if write_half.write_all(&bytes).await.is_err() { break; }
        }
    });

    while let Some(line) = reader.next_line().await? {
        let req: ClientRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(DaemonEvent::Error { session_id: None, message: format!("bad request: {e}") });
                continue;
            }
        };
        route_request(req, &tx, &sessions, &inbox).await;
    }
    drop(tx);
    let _ = write_task.await;
    Ok(())
}

async fn route_request(
    req: ClientRequest,
    tx: &mpsc::UnboundedSender<DaemonEvent>,
    sessions: &SessionRegistry,
    inbox: &Inbox,
) {
    match req {
        ClientRequest::Ping => { let _ = tx.send(DaemonEvent::Pong); }
        ClientRequest::OpenSession { cwd } => {
            let session_id = sessions.open(PathBuf::from(cwd), tx.clone()).await;
            let _ = inbox.push(&InboxEvent::ClientConnected { session_id });
            let _ = tx.send(DaemonEvent::SessionOpened { session_id });
        }
        ClientRequest::UserMessage { session_id, text } => {
            let Some(state) = sessions.get(session_id).await else {
                let _ = tx.send(DaemonEvent::Error {
                    session_id: Some(session_id),
                    message: "unknown session".to_string(),
                });
                return;
            };
            let _ = inbox.push(&InboxEvent::UserMessage { session_id, text: text.clone() });
            // Spawn the turn — stub for v0a Task 6; real cpu integration in Task 11.
            tokio::spawn(async move {
                let _ = handle_user_message_stub(state, text).await;
            });
        }
        ClientRequest::CloseSession { session_id } => {
            let _ = inbox.push(&InboxEvent::ClientDisconnected { session_id });
            sessions.close(session_id).await;
        }
    }
}
