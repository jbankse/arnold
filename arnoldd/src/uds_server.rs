use std::path::Path;
use anyhow::Result;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use arnold_wire::{ClientRequest, DaemonEvent, ProviderStatus};
use crate::session::{DaemonState, handle_user_message};
use crate::inbox::{Inbox, InboxEvent};
use crate::secrets::Secrets;

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
            ClientRequest::SetProviderKey { provider, key } => {
                let path = match Secrets::default_path() {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = tx.send(DaemonEvent::Error { session_id: None, message: format!("secrets path error: {e}") });
                        continue;
                    }
                };
                {
                    let mut s = state.secrets.write().await;
                    s.set(&provider, key);
                    if let Err(e) = s.save(&path) {
                        let _ = tx.send(DaemonEvent::Error { session_id: None, message: format!("failed to persist secrets: {e}") });
                        continue;
                    }
                }
                let _ = tx.send(secrets_status_event(&*state.secrets.read().await));
            }
            ClientRequest::GetSecretsStatus => {
                let _ = tx.send(secrets_status_event(&*state.secrets.read().await));
            }
        }
    }
    drop(tx);
    let _ = write_task.await;
    Ok(())
}

/// Build a SecretsStatus event from the current Secrets without echoing key
/// values. Lists each known provider (anthropic/openai/xai) plus any extras,
/// with `configured: true` iff the stored value is non-empty.
fn secrets_status_event(s: &Secrets) -> DaemonEvent {
    let mut providers = Vec::new();
    for p in ["anthropic", "openai", "xai"] {
        providers.push(ProviderStatus { provider: p.into(), configured: !s.get(p).is_empty() });
    }
    for (k, v) in &s.extras {
        // Extras are stored as `<provider>_api_key`; strip the suffix for the user-facing name.
        let name = k.strip_suffix("_api_key").unwrap_or(k);
        providers.push(ProviderStatus { provider: name.into(), configured: !v.is_empty() });
    }
    DaemonEvent::SecretsStatus { providers }
}
