use std::path::Path;
use anyhow::Result;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use arnold_wire::{ClientRequest, DaemonEvent};

/// Bind a UDS listener at `path`, removing any stale socket first.
pub async fn bind(path: &Path) -> Result<UnixListener> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    // 0600
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Handle one client connection. v0a stub: respond to Ping; reject everything else.
pub async fn handle_connection(stream: UnixStream) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half).lines();
    while let Some(line) = reader.next_line().await? {
        let req: ClientRequest = serde_json::from_str(&line)?;
        let resp = match req {
            ClientRequest::Ping => DaemonEvent::Pong,
            _ => DaemonEvent::Error {
                session_id: None,
                message: "session lifecycle not yet implemented (task 6)".to_string(),
            },
        };
        let mut bytes = serde_json::to_vec(&resp)?;
        bytes.push(b'\n');
        write_half.write_all(&bytes).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn ping_pong_round_trip() {
        let tmp = TempDir::new().unwrap();
        let sock = tmp.path().join("test.sock");
        let listener = bind(&sock).await.unwrap();

        // Server task: accept one connection and handle.
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream).await.unwrap();
        });

        // Client side.
        let mut client = UnixStream::connect(&sock).await.unwrap();
        let req = ClientRequest::Ping;
        let mut bytes = serde_json::to_vec(&req).unwrap();
        bytes.push(b'\n');
        client.write_all(&bytes).await.unwrap();
        client.shutdown().await.unwrap();

        let mut buf = String::new();
        client.read_to_string(&mut buf).await.unwrap();
        let resp: DaemonEvent = serde_json::from_str(buf.trim()).unwrap();
        assert!(matches!(resp, DaemonEvent::Pong));

        server.await.unwrap();
    }
}
