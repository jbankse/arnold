use std::path::Path;
use anyhow::Result;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use arnold_wire::{ClientRequest, DaemonEvent};
use crate::session::{DaemonState, handle_user_message};
use crate::inbox::{Inbox, InboxEvent};

pub async fn bind(path: &Path) -> Result<UnixListener> {
    if path.exists() { std::fs::remove_file(path)?; }
    let listener = UnixListener::bind(path)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub async fn handle_connection(stream: UnixStream, state: DaemonState, inbox: Inbox) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half).lines();
    let (tx, mut rx) = mpsc::unbounded_channel::<DaemonEvent>();

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
            Err(e) => { let _ = tx.send(DaemonEvent::Error { session_id: None, message: format!("bad request: {e}") }); continue; }
        };
        match req {
            ClientRequest::Ping => { let _ = tx.send(DaemonEvent::Pong); }
            ClientRequest::OpenSession { cwd } => {
                let session_id = state.open(std::path::PathBuf::from(cwd), tx.clone()).await;
                let _ = inbox.push(&InboxEvent::ClientConnected { session_id });
                let _ = tx.send(DaemonEvent::SessionOpened { session_id });
            }
            ClientRequest::UserMessage { session_id, text } => {
                let Some(session) = state.get(session_id).await else {
                    let _ = tx.send(DaemonEvent::Error { session_id: Some(session_id), message: "unknown session".into() });
                    continue;
                };
                let _ = inbox.push(&InboxEvent::UserMessage { session_id, text: text.clone() });
                let s = state.clone();
                let tx_for_err = tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_user_message(s, session, text).await {
                        let _ = tx_for_err.send(DaemonEvent::Error { session_id: Some(session_id), message: format!("turn error: {e}") });
                    }
                });
            }
            ClientRequest::CloseSession { session_id } => {
                let _ = inbox.push(&InboxEvent::ClientDisconnected { session_id });
                state.close(session_id).await;
            }
        }
    }
    drop(tx);
    let _ = write_task.await;
    Ok(())
}
