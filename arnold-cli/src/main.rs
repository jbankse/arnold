use anyhow::Result;
use arnold_wire::{ClientRequest, DaemonEvent};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

fn socket_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))?;
    Ok(home.join(".arnold/arnold.sock"))
}

async fn send_request(stream: &mut UnixStream, req: &ClientRequest) -> Result<()> {
    let mut bytes = serde_json::to_vec(req)?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let sock = socket_path()?;
    let mut stream = UnixStream::connect(&sock).await
        .map_err(|e| anyhow::anyhow!("cannot connect to {} ({e}); is arnoldd running?", sock.display()))?;

    let cwd = std::env::current_dir()?.to_string_lossy().into_owned();
    send_request(&mut stream, &ClientRequest::OpenSession { cwd }).await?;

    let (read_half, mut write_half) = stream.into_split();
    let mut server_lines = BufReader::new(read_half).lines();

    // Read SessionOpened first
    let line = server_lines.next_line().await?.ok_or_else(|| anyhow::anyhow!("daemon closed"))?;
    let ev: DaemonEvent = serde_json::from_str(&line)?;
    let session_id = match ev {
        DaemonEvent::SessionOpened { session_id } => session_id,
        other => anyhow::bail!("expected SessionOpened, got {other:?}"),
    };

    println!("arnold session {session_id} (Ctrl-D to exit)\n");

    // Server-event reader task
    let server_task = tokio::spawn(async move {
        while let Ok(Some(line)) = server_lines.next_line().await {
            let Ok(ev): Result<DaemonEvent, _> = serde_json::from_str(&line) else { continue };
            match ev {
                DaemonEvent::Reply { text, .. } => println!("\narnold: {text}"),
                DaemonEvent::TurnComplete { .. } => print!("\n> "),
                DaemonEvent::Error { message, .. } => eprintln!("\n[error] {message}"),
                _ => {}
            }
            use tokio::io::AsyncWriteExt;
            let _ = tokio::io::stdout().flush().await;
        }
    });

    // Stdin reader: send UserMessage frames
    let stdin = tokio::io::stdin();
    let mut stdin_lines = BufReader::new(stdin).lines();
    print!("> "); use std::io::Write; std::io::stdout().flush()?;
    while let Some(line) = stdin_lines.next_line().await? {
        if line.trim().is_empty() { print!("> "); std::io::stdout().flush()?; continue; }
        let req = ClientRequest::UserMessage { session_id, text: line };
        let mut bytes = serde_json::to_vec(&req)?;
        bytes.push(b'\n');
        write_half.write_all(&bytes).await?;
    }

    // EOF on stdin → close
    let req = ClientRequest::CloseSession { session_id };
    let mut bytes = serde_json::to_vec(&req)?;
    bytes.push(b'\n');
    let _ = write_half.write_all(&bytes).await;
    drop(write_half);
    let _ = server_task.await;
    Ok(())
}
