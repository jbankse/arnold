# Arnold v0a — Standalone Assistant Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up Arnold as a functioning standalone Claude-Code-style assistant — persistent Rust daemon + interim Rust CLI client + 439-`cpu` subprocess integration + syscall harness + markdown memory + SQLite inbox — without 439 dispatch (`sys_spin_up_439`, in-flight jobs, BIOS subprocess) or the Go TUI client (both shipped in v0b).

**Architecture:** A persistent `arnoldd` Rust daemon listens on a Unix domain socket and accepts session connections from an `arnold` CLI client. Per session, the daemon spawns 439's `cpu` binary as a subprocess and exchanges JSON-line frames with it. Arnold builds its own plan frame (role=`task`, task_type=`chat`, Arnold's syscall menu, Arnold's role payload, Arnold's generated constraint schema) so `cpu` is reused unmodified. Syscall results are produced by Arnold's own handlers (file, shell, web, memory, conversation). State lives in `~/.arnold/`: SQLite inbox, markdown memory, optional workspace.

**Tech Stack:** Rust 1.75+ (Cargo workspace), Tokio async runtime, `rusqlite` (sync SQLite via `spawn_blocking`), `schemars` + `serde_json` (typed syscalls + JSON), `tokio_util::codec::LinesCodec` (newline-delimited framing), `reqwest` (web fetch), `ripgrep` shell-out (search), 439's prebuilt `cpu` binary (built from `vendor/439` submodule at install time, statically links `providers` C++ lib).

---

## File Structure

Arnold v0a creates one Cargo workspace at the repo root with three member crates and a small set of shared files. Each crate has one focused responsibility:

```
Codebases/Arnold/
├── .gitignore
├── .gitmodules                # vendor/439 submodule
├── Cargo.toml                 # workspace manifest
├── README.md                  # one-paragraph orientation
├── CLAUDE.md                  # operating manual for AI agents in this repo
├── AGENTS.md                  # repo architecture for agents
│
├── arnold-wire/               # crate: shared wire types (daemon ↔ CLI client)
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs             # ClientRequest, DaemonEvent enums (serde)
│
├── arnoldd/                   # crate: persistent daemon binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs            # entry, signal handling, daemon bootstrap
│       ├── config.rs          # ArnoldConfig load from ~/.arnold/config.toml
│       ├── uds_server.rs      # UDS listener, per-conn dispatch
│       ├── session.rs         # per-session state + lifecycle
│       ├── inbox.rs           # SQLite-backed event store
│       ├── memory.rs          # markdown memory store + MEMORY.md index
│       ├── jail.rs            # path-jail helpers (resolve, validate)
│       ├── syscall.rs         # Arnold's typed Syscall enum + serde rename
│       ├── schema.rs          # constraint_schema generator (schemars-based)
│       ├── plan_frame.rs      # builds the cpu plan frame for Arnold's role
│       ├── cpu_process.rs     # spawns 439's cpu, JSON-line frame loop
│       ├── handler.rs         # syscall handler trait + dispatch
│       └── handlers/
│           ├── mod.rs
│           ├── lifecycle.rs   # sys_reply, sys_done
│           ├── file.rs        # sys_read_file, sys_write_file, sys_replace_in_file, sys_list_dir, sys_search
│           ├── shell.rs       # sys_run_command
│           ├── web.rs         # sys_web_fetch
│           └── memory_syscalls.rs  # sys_memory_read, sys_memory_write, sys_memory_list
│
├── arnold-cli/                # crate: interim Rust REPL client (replaced by Go TUI in v0b)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs            # UDS connect, stdin REPL, prints daemon events
│
├── role/
│   └── ROLE.md                # Arnold's system-prompt body (role payload body)
│
├── vendor/
│   └── 439/                   # git submodule pinned to a 439 release tag
│
├── scripts/
│   ├── build-cpu.sh           # cmake-builds 439's cpu binary, copies to ~/.arnold/bin/
│   ├── install.sh             # builds workspace, installs binaries, creates ~/.arnold/
│   └── uninstall.sh
│
└── docs/superpowers/
    ├── specs/2026-05-20-arnold-personal-agent-design.md  # already committed
    └── plans/2026-05-20-arnold-v0a-standalone-assistant.md  # this plan
```

**Rationale for the split:**
- `arnold-wire` is its own crate so the daemon and the CLI client share exactly one definition of the wire types — no drift.
- Syscall handlers live in their own module tree (`handlers/`) so each family is a small focused file. Adding `sys_spin_up_439` in v0b becomes "new module under handlers/" without touching existing ones.
- `cpu_process.rs` is isolated so the 439-cpu protocol contract is in one place; if 439's frame shape changes, exactly one file updates.
- The interim `arnold-cli` is its own crate so we delete it cleanly in v0b when the Go TUI replaces it.

---

## Task 1: Repo bootstrap, Cargo workspace, 439 submodule, agent operating manuals

**Files:**
- Create: `Cargo.toml` (workspace manifest)
- Create: `.gitignore`
- Create: `.gitmodules` + `vendor/439/` submodule
- Create: `README.md`
- Create: `CLAUDE.md`
- Create: `AGENTS.md`
- Create: `scripts/build-cpu.sh`

- [ ] **Step 1: Add `vendor/439` as a git submodule**

```bash
cd /Users/joshua/Codebases/Arnold
git submodule add https://github.com/jbankse/439 vendor/439
cd vendor/439 && git checkout main && cd ../..
```

Expected: `.gitmodules` exists; `vendor/439/Cargo.toml` exists.

- [ ] **Step 2: Write workspace `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["arnold-wire", "arnoldd", "arnold-cli"]

[workspace.package]
edition = "2021"
rust-version = "1.75"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["codec"] }
anyhow = "1"
thiserror = "1"
schemars = "0.8"
rusqlite = { version = "0.31", features = ["bundled"] }
reqwest = { version = "0.12", features = ["rustls-tls"], default-features = false }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
toml = "0.8"
dirs = "5"
```

- [ ] **Step 3: Write `.gitignore`**

```
target/
*.swp
.DS_Store
~/.arnold/
.envrc
.cargo/
```

- [ ] **Step 4: Write `README.md` (one paragraph)**

```markdown
# Arnold

A long-running, code-focused personal agent that lives as a persistent daemon. Spawns ephemeral [439](https://github.com/jbankse/439) environments as a native syscall capability while staying responsive to ongoing conversation.

Design spec: [docs/superpowers/specs/2026-05-20-arnold-personal-agent-design.md](docs/superpowers/specs/2026-05-20-arnold-personal-agent-design.md)

v0a implementation plan: [docs/superpowers/plans/2026-05-20-arnold-v0a-standalone-assistant.md](docs/superpowers/plans/2026-05-20-arnold-v0a-standalone-assistant.md)
```

- [ ] **Step 5: Write `CLAUDE.md` (operating manual for AI agents working in this repo)**

```markdown
# Arnold — Operating Manual for AI Agents

Read first:
1. README.md
2. docs/superpowers/specs/2026-05-20-arnold-personal-agent-design.md (the spec)
3. docs/superpowers/plans/ (active plan)

This repo follows the same naming and contribution discipline as the [439 codebase](https://github.com/jbankse/439): conventional commits, per-crate cargo discipline (`cargo check -p <crate>`, not workspace-wide), snake_case files, branch-per-change.

The `vendor/439` submodule is a black-box dependency; do not edit files inside it. Bump the pinned ref with `git submodule update --remote vendor/439` when you intentionally pull in a new 439 version, and capture the bump in a `chore(vendor): bump 439 to <sha>` commit.
```

- [ ] **Step 6: Write `AGENTS.md` (mirror of CLAUDE.md for non-Claude agents)**

```markdown
# Arnold — Agent Operating Guide

See CLAUDE.md. Same rules apply.
```

- [ ] **Step 7: Write `scripts/build-cpu.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR="$HERE/../vendor/439"
DEST_DIR="${1:-$HOME/.arnold/bin}"

mkdir -p "$DEST_DIR"
cd "$VENDOR"
cmake -G Ninja -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --target cpu
cp build/cpu/cpu "$DEST_DIR/cpu"
echo "Installed cpu binary to $DEST_DIR/cpu"
```

Make it executable:

```bash
chmod +x scripts/build-cpu.sh
```

- [ ] **Step 8: Verify workspace builds (empty crates not yet created — verifies just the toml)**

Run: `cargo check --workspace --offline 2>&1 | head -5`

Expected: errors complaining about missing members (`arnold-wire`, `arnoldd`, `arnold-cli`) — that's fine, we create them in Task 2+. No TOML syntax errors.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml .gitignore .gitmodules vendor/439 README.md CLAUDE.md AGENTS.md scripts/build-cpu.sh
git commit -m "chore: bootstrap workspace, vendor 439 submodule, add agent operating manuals"
```

---

## Task 2: Shared wire-types crate (`arnold-wire`)

**Files:**
- Create: `arnold-wire/Cargo.toml`
- Create: `arnold-wire/src/lib.rs`

- [ ] **Step 1: Write `arnold-wire/Cargo.toml`**

```toml
[package]
name = "arnold-wire"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
uuid.workspace = true
chrono.workspace = true
```

- [ ] **Step 2: Write the failing test for round-trip JSON encoding of wire types**

Create `arnold-wire/src/lib.rs`:

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Client → Daemon: anything the CLI client can send.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientRequest {
    /// Open a new session against a given working directory.
    OpenSession { cwd: String },
    /// User typed something in the current session.
    UserMessage { session_id: Uuid, text: String },
    /// Cleanly close the current session.
    CloseSession { session_id: Uuid },
    /// Lightweight health check.
    Ping,
}

/// Daemon → Client: anything the daemon can push back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    SessionOpened { session_id: Uuid },
    /// Arnold's `sys_reply` text routed to this client.
    Reply { session_id: Uuid, text: String },
    /// Arnold finished its turn and is waiting on the inbox.
    TurnComplete { session_id: Uuid },
    /// Non-fatal error message for the user.
    Error { session_id: Option<Uuid>, message: String },
    Pong,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_request_round_trip() {
        let req = ClientRequest::UserMessage {
            session_id: Uuid::nil(),
            text: "hello".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ClientRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, parsed);
        assert!(json.contains("\"type\":\"user_message\""));
    }

    #[test]
    fn daemon_event_round_trip() {
        let ev = DaemonEvent::Reply {
            session_id: Uuid::nil(),
            text: "hi".to_string(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, parsed);
        assert!(json.contains("\"type\":\"reply\""));
    }
}
```

- [ ] **Step 3: Run the tests and confirm they pass**

Run: `cargo test -p arnold-wire`

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add arnold-wire/
git commit -m "feat(wire): define ClientRequest / DaemonEvent shared types"
```

---

## Task 3: Daemon scaffold + UDS server

**Files:**
- Create: `arnoldd/Cargo.toml`
- Create: `arnoldd/src/main.rs`
- Create: `arnoldd/src/uds_server.rs`
- Create: `arnoldd/src/config.rs`

- [ ] **Step 1: Write `arnoldd/Cargo.toml`**

```toml
[package]
name = "arnoldd"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
arnold-wire = { path = "../arnold-wire" }
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tokio-util.workspace = true
anyhow.workspace = true
thiserror.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
uuid.workspace = true
chrono.workspace = true
dirs.workspace = true
toml.workspace = true
futures = "0.3"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Write `arnoldd/src/config.rs`**

```rust
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArnoldConfig {
    /// Path to the 439 cpu binary. Defaults to ~/.arnold/bin/cpu
    pub cpu_binary: PathBuf,
    /// LLM provider (anthropic, openai, etc.)
    pub llm_provider: String,
    /// Model identifier
    pub model: String,
    /// Additional directories Arnold's jail will allow beyond ~/.arnold/workspace
    pub allowed_dirs: Vec<PathBuf>,
}

impl Default for ArnoldConfig {
    fn default() -> Self {
        let home = dirs::home_dir().expect("no home dir");
        Self {
            cpu_binary: home.join(".arnold/bin/cpu"),
            llm_provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            allowed_dirs: vec![],
        }
    }
}

impl ArnoldConfig {
    pub fn load_or_default() -> anyhow::Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))?;
        let path = home.join(".arnold/config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&text)?)
    }

    pub fn arnold_dir() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))?;
        let dir = home.join(".arnold");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn socket_path() -> anyhow::Result<PathBuf> {
        Ok(Self::arnold_dir()?.join("arnold.sock"))
    }
}
```

- [ ] **Step 3: Write the failing test for the UDS server**

Create `arnoldd/src/uds_server.rs`:

```rust
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
```

- [ ] **Step 4: Write `arnoldd/src/main.rs` (minimal entry, accepts connections, dispatches)**

```rust
mod config;
mod uds_server;

use anyhow::Result;
use tracing::{info, error};
use config::ArnoldConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("arnoldd=info".parse()?))
        .init();

    let _config = ArnoldConfig::load_or_default()?;
    let sock_path = ArnoldConfig::socket_path()?;
    info!(socket = %sock_path.display(), "arnoldd starting");

    let listener = uds_server::bind(&sock_path).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = uds_server::handle_connection(stream).await {
                error!("connection handler error: {e:#}");
            }
        });
    }
}
```

- [ ] **Step 5: Run tests and confirm green**

Run: `cargo test -p arnoldd`

Expected: 1 passed (`ping_pong_round_trip`).

- [ ] **Step 6: Verify the daemon builds and starts**

```bash
cargo run -p arnoldd &
DAEMON_PID=$!
sleep 1
ls -la ~/.arnold/arnold.sock
kill $DAEMON_PID
```

Expected: socket file exists with permissions `srw-------`.

- [ ] **Step 7: Commit**

```bash
git add arnoldd/
git commit -m "feat(arnoldd): UDS server scaffold with ping/pong, config loader"
```

---

## Task 4: Interim Rust CLI client (`arnold-cli`)

**Files:**
- Create: `arnold-cli/Cargo.toml`
- Create: `arnold-cli/src/main.rs`

This client is the smallest possible interactive frontend so v0a is usable end-to-end. It will be replaced wholesale by the Go TUI in v0b — keep it simple.

- [ ] **Step 1: Write `arnold-cli/Cargo.toml`**

```toml
[package]
name = "arnold-cli"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "arnold"
path = "src/main.rs"

[dependencies]
arnold-wire = { path = "../arnold-wire" }
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
anyhow.workspace = true
dirs.workspace = true
uuid.workspace = true
```

- [ ] **Step 2: Write `arnold-cli/src/main.rs`**

```rust
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
```

- [ ] **Step 3: Verify the client builds**

Run: `cargo build -p arnold-cli`

Expected: builds clean.

- [ ] **Step 4: Verify end-to-end ping path (run daemon in one terminal, client in another)**

Terminal A: `cargo run -p arnoldd`
Terminal B: `cargo run -p arnold-cli` and type a single line + Ctrl-D.

Expected: client receives an `Error` event saying "session lifecycle not yet implemented" — the wire path works; session lifecycle is Task 6.

- [ ] **Step 5: Commit**

```bash
git add arnold-cli/
git commit -m "feat(cli): interim Rust REPL client (replaced by Go TUI in v0b)"
```

---

## Task 5: SQLite inbox

**Files:**
- Create: `arnoldd/src/inbox.rs`
- Modify: `arnoldd/src/main.rs` (register inbox in shared state)

- [ ] **Step 1: Write the inbox module with schema + repo**

Create `arnoldd/src/inbox.rs`:

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboxEvent {
    UserMessage { session_id: Uuid, text: String },
    ClientConnected { session_id: Uuid },
    ClientDisconnected { session_id: Uuid },
}

#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub id: i64,
    pub event: InboxEvent,
    pub created_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct Inbox(Arc<Mutex<Connection>>);

impl Inbox {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            CREATE TABLE IF NOT EXISTS inbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                consumed_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_inbox_unconsumed
                ON inbox(consumed_at) WHERE consumed_at IS NULL;
            "#,
        )?;
        Ok(Self(Arc::new(Mutex::new(conn))))
    }

    pub fn push(&self, event: &InboxEvent) -> Result<i64> {
        let conn = self.0.lock().unwrap();
        let kind = match event {
            InboxEvent::UserMessage { .. } => "user_message",
            InboxEvent::ClientConnected { .. } => "client_connected",
            InboxEvent::ClientDisconnected { .. } => "client_disconnected",
        };
        let payload = serde_json::to_string(event)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO inbox (kind, payload_json, created_at) VALUES (?1, ?2, ?3)",
            params![kind, payload, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn pending_for_session(&self, session_id: Uuid) -> Result<Vec<StoredEvent>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, payload_json, created_at FROM inbox
             WHERE consumed_at IS NULL ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let payload: String = row.get(1)?;
            let created_at: String = row.get(2)?;
            Ok((id, payload, created_at))
        })?;
        let mut out = vec![];
        for r in rows {
            let (id, payload, created_at) = r?;
            let event: InboxEvent = serde_json::from_str(&payload)?;
            let session_matches = match &event {
                InboxEvent::UserMessage { session_id: s, .. } => *s == session_id,
                InboxEvent::ClientConnected { session_id: s } => *s == session_id,
                InboxEvent::ClientDisconnected { session_id: s } => *s == session_id,
            };
            if !session_matches { continue; }
            out.push(StoredEvent {
                id,
                event,
                created_at: DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc),
                consumed_at: None,
            });
        }
        Ok(out)
    }

    pub fn mark_consumed(&self, id: i64) -> Result<()> {
        let conn = self.0.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute("UPDATE inbox SET consumed_at = ?1 WHERE id = ?2", params![now, id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn push_pending_consume_round_trip() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("inbox.db");
        let inbox = Inbox::open(&db).unwrap();

        let sid = Uuid::new_v4();
        inbox.push(&InboxEvent::UserMessage { session_id: sid, text: "hi".into() }).unwrap();
        inbox.push(&InboxEvent::ClientConnected { session_id: sid }).unwrap();

        let pending = inbox.pending_for_session(sid).unwrap();
        assert_eq!(pending.len(), 2);

        inbox.mark_consumed(pending[0].id).unwrap();
        let remaining = inbox.pending_for_session(sid).unwrap();
        assert_eq!(remaining.len(), 1);
    }
}
```

- [ ] **Step 2: Register inbox module in `arnoldd/src/main.rs`**

Add at the top of `main.rs`:

```rust
mod inbox;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p arnoldd inbox`

Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add arnoldd/src/inbox.rs arnoldd/src/main.rs
git commit -m "feat(arnoldd): SQLite-backed inbox with push/pending/consume"
```

---

## Task 6: Per-session lifecycle (OpenSession → cpu spawn placeholder → reply path)

**Files:**
- Create: `arnoldd/src/session.rs`
- Modify: `arnoldd/src/uds_server.rs` (route requests to sessions)
- Modify: `arnoldd/src/main.rs` (shared session registry)

This task wires the session lifecycle skeleton. The actual `cpu` subprocess spawn lands in Task 11; for now Arnold echoes user messages back as a stubbed reply so the client→daemon→client path is provable.

- [ ] **Step 1: Write `arnoldd/src/session.rs`**

```rust
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
```

- [ ] **Step 2: Update `arnoldd/src/uds_server.rs` to route requests via SessionRegistry**

Replace the contents of `uds_server.rs`:

```rust
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
```

- [ ] **Step 3: Update `arnoldd/src/main.rs` to wire SessionRegistry + Inbox**

Replace `main.rs`:

```rust
mod config;
mod inbox;
mod session;
mod uds_server;

use anyhow::Result;
use tracing::{info, error};
use config::ArnoldConfig;
use inbox::Inbox;
use session::SessionRegistry;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("arnoldd=info".parse()?))
        .init();

    let _config = ArnoldConfig::load_or_default()?;
    let sock_path = ArnoldConfig::socket_path()?;
    let inbox = Inbox::open(&ArnoldConfig::arnold_dir()?.join("inbox.db"))?;
    let sessions = SessionRegistry::default();

    info!(socket = %sock_path.display(), "arnoldd starting");
    let listener = uds_server::bind(&sock_path).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let s = sessions.clone();
        let i = inbox.clone();
        tokio::spawn(async move {
            if let Err(e) = uds_server::handle_connection(stream, s, i).await {
                error!("connection handler error: {e:#}");
            }
        });
    }
}
```

- [ ] **Step 4: End-to-end manual verification**

Terminal A: `cargo run -p arnoldd`
Terminal B: `cargo run -p arnold-cli` — type "hello", press Enter, then Ctrl-D.

Expected: CLI prints `arnold: echo (stub): hello`. Daemon logs show session open + close.

- [ ] **Step 5: Commit**

```bash
git add arnoldd/src/session.rs arnoldd/src/uds_server.rs arnoldd/src/main.rs
git commit -m "feat(arnoldd): per-session lifecycle, stub echo turn handler"
```

---

## Task 7: Markdown memory store

**Files:**
- Create: `arnoldd/src/memory.rs`
- Modify: `arnoldd/src/main.rs`

- [ ] **Step 1: Write the memory module with tests**

Create `arnoldd/src/memory.rs`:

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    User,
    Feedback,
    Project,
    Reference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTopic {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub kind: MemoryType,
    pub body: String,
}

#[derive(Clone)]
pub struct MemoryStore {
    root: PathBuf,
}

impl MemoryStore {
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let index = root.join("MEMORY.md");
        if !index.exists() {
            std::fs::write(&index, "# Arnold Memory\n\n_No entries yet._\n")?;
        }
        Ok(Self { root: root.to_path_buf() })
    }

    pub fn index(&self) -> Result<String> {
        Ok(std::fs::read_to_string(self.root.join("MEMORY.md"))?)
    }

    pub fn read(&self, topic: &str) -> Result<MemoryTopic> {
        let path = self.topic_path(topic);
        let text = std::fs::read_to_string(&path)?;
        parse_topic(&text)
    }

    pub fn write(&self, topic: &str, content: &MemoryTopic) -> Result<()> {
        let path = self.topic_path(topic);
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(&path, serialize_topic(content))?;
        self.rebuild_index()?;
        Ok(())
    }

    fn topic_path(&self, topic: &str) -> PathBuf {
        // Disallow path traversal at this layer; jail layer (Task 8) also checks.
        let safe: String = topic.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '/').collect();
        self.root.join(format!("{safe}.md"))
    }

    fn rebuild_index(&self) -> Result<()> {
        let mut lines = vec!["# Arnold Memory".to_string(), String::new()];
        for entry in walkdir::WalkDir::new(&self.root).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if !p.is_file() { continue; }
            if p.file_name().and_then(|n| n.to_str()) == Some("MEMORY.md") { continue; }
            if p.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
            let rel = p.strip_prefix(&self.root).unwrap_or(p);
            let text = std::fs::read_to_string(p).unwrap_or_default();
            let topic = parse_topic(&text).ok();
            let (name, desc) = match topic {
                Some(t) => (t.name, t.description),
                None => (rel.display().to_string(), "(unparsed)".to_string()),
            };
            lines.push(format!("- [{}]({}) — {}", name, rel.display(), desc));
        }
        lines.push(String::new());
        std::fs::write(self.root.join("MEMORY.md"), lines.join("\n"))?;
        Ok(())
    }
}

fn parse_topic(text: &str) -> Result<MemoryTopic> {
    let stripped = text.strip_prefix("---\n").ok_or_else(|| anyhow::anyhow!("missing frontmatter"))?;
    let end = stripped.find("\n---\n").ok_or_else(|| anyhow::anyhow!("unterminated frontmatter"))?;
    let frontmatter = &stripped[..end];
    let body = &stripped[end + 5..];

    #[derive(Deserialize)]
    struct Fm { name: String, description: String, #[serde(rename = "type")] kind: MemoryType }
    let fm: Fm = serde_yaml::from_str(frontmatter)?;

    Ok(MemoryTopic {
        name: fm.name,
        description: fm.description,
        kind: fm.kind,
        body: body.to_string(),
    })
}

fn serialize_topic(t: &MemoryTopic) -> String {
    let kind = serde_json::to_value(&t.kind).unwrap().as_str().unwrap().to_string();
    format!(
        "---\nname: {}\ndescription: {}\ntype: {}\n---\n{}",
        t.name, t.description, kind, t.body
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_then_read_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = MemoryStore::open(tmp.path()).unwrap();
        let topic = MemoryTopic {
            name: "Test Topic".into(),
            description: "describes the topic".into(),
            kind: MemoryType::User,
            body: "body text here\n".into(),
        };
        store.write("test_topic", &topic).unwrap();
        let read = store.read("test_topic").unwrap();
        assert_eq!(read.name, "Test Topic");
        assert_eq!(read.kind, MemoryType::User);

        let index = store.index().unwrap();
        assert!(index.contains("Test Topic"));
    }
}
```

- [ ] **Step 2: Add `walkdir` and `serde_yaml` to `arnoldd/Cargo.toml`**

In `[dependencies]`:

```toml
walkdir = "2"
serde_yaml = "0.9"
```

- [ ] **Step 3: Add module to `main.rs`**

```rust
mod memory;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p arnoldd memory`

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add arnoldd/src/memory.rs arnoldd/src/main.rs arnoldd/Cargo.toml
git commit -m "feat(arnoldd): markdown memory store with auto-rebuilt MEMORY.md index"
```

---

## Task 8: Path jail

**Files:**
- Create: `arnoldd/src/jail.rs`
- Modify: `arnoldd/src/main.rs`

The jail mirrors the patterns in `vendor/439/os/kernel/src/path_jail.rs`. Reject `..`, absolute paths, tilde expansion; resolve against an allowlist of root directories.

- [ ] **Step 1: Write the jail module**

Create `arnoldd/src/jail.rs`:

```rust
use anyhow::{anyhow, Result};
use std::path::{Component, Path, PathBuf};

#[derive(Clone)]
pub struct Jail {
    roots: Vec<PathBuf>,
}

impl Jail {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        let roots = roots.into_iter().map(|p| p.canonicalize().unwrap_or(p)).collect();
        Self { roots }
    }

    pub fn add_root(&mut self, root: PathBuf) {
        let canon = root.canonicalize().unwrap_or(root);
        if !self.roots.contains(&canon) { self.roots.push(canon); }
    }

    /// Resolve `requested` (workspace-relative — no `..`, no leading `/`, no `~`) against
    /// the first root that contains it. Returns an absolute canonical path.
    pub fn resolve(&self, requested: &str) -> Result<PathBuf> {
        let p = Path::new(requested);
        for comp in p.components() {
            match comp {
                Component::ParentDir => return Err(anyhow!("path component '..' is forbidden")),
                Component::RootDir => return Err(anyhow!("absolute paths are forbidden")),
                Component::Prefix(_) => return Err(anyhow!("path prefix is forbidden")),
                _ => {}
            }
        }
        if requested.starts_with('~') { return Err(anyhow!("'~' expansion is forbidden")); }

        // Try each root in order; resolve and check containment.
        for root in &self.roots {
            let candidate = root.join(p);
            // Canonicalize parent so symlink escapes are caught.
            let parent_canon = candidate.parent()
                .map(|pp| pp.canonicalize().unwrap_or_else(|_| pp.to_path_buf()))
                .unwrap_or_else(|| root.clone());
            if parent_canon.starts_with(root) {
                return Ok(candidate);
            }
        }
        Err(anyhow!("path '{requested}' escapes all jail roots"))
    }

    pub fn validate_inside_any(&self, abs: &Path) -> Result<()> {
        let canon = abs.canonicalize().unwrap_or_else(|_| abs.to_path_buf());
        for root in &self.roots {
            if canon.starts_with(root) { return Ok(()); }
        }
        Err(anyhow!("path '{}' is outside jail", abs.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn forbids_parent_dir() {
        let tmp = TempDir::new().unwrap();
        let jail = Jail::new(vec![tmp.path().to_path_buf()]);
        assert!(jail.resolve("../etc/passwd").is_err());
    }

    #[test]
    fn forbids_absolute() {
        let tmp = TempDir::new().unwrap();
        let jail = Jail::new(vec![tmp.path().to_path_buf()]);
        assert!(jail.resolve("/etc/passwd").is_err());
    }

    #[test]
    fn resolves_simple_relative() {
        let tmp = TempDir::new().unwrap();
        let jail = Jail::new(vec![tmp.path().to_path_buf()]);
        let p = jail.resolve("foo/bar.txt").unwrap();
        assert!(p.starts_with(tmp.path()));
        assert!(p.ends_with("foo/bar.txt"));
    }
}
```

- [ ] **Step 2: Register module in `main.rs`**

```rust
mod jail;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p arnoldd jail`

Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add arnoldd/src/jail.rs arnoldd/src/main.rs
git commit -m "feat(arnoldd): path-jail module patterned on 439, with ParentDir/abs/tilde guards"
```

---

## Task 9: Typed syscall enum + constraint-schema generator

**Files:**
- Create: `arnoldd/src/syscall.rs`
- Create: `arnoldd/src/schema.rs`
- Modify: `arnoldd/src/main.rs`

This task defines Arnold's own syscall vocabulary (separate from 439's). The schema generator produces a JSON Schema `oneOf` over allowed methods — same shape `cpu` expects in the plan frame's `constraint_schema` field.

- [ ] **Step 1: Write `arnoldd/src/syscall.rs` with all v0a syscalls**

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Syscall {
    // Conversation lifecycle
    #[serde(rename = "sys_reply")]
    Reply { text: String },
    #[serde(rename = "sys_done")]
    Done {},

    // File / project
    #[serde(rename = "sys_read_file")]
    ReadFile { path: String },
    #[serde(rename = "sys_write_file")]
    WriteFile { path: String, contents: String },
    #[serde(rename = "sys_replace_in_file")]
    ReplaceInFile { path: String, old_string: String, new_string: String },
    #[serde(rename = "sys_list_dir")]
    ListDir { path: String },
    #[serde(rename = "sys_search")]
    Search { query: String, path: Option<String>, glob: Option<String> },

    // Shell
    #[serde(rename = "sys_run_command")]
    RunCommand {
        cmd: String,
        #[serde(default)] args: Vec<String>,
        #[serde(default)] cwd: Option<String>,
        #[serde(default)] timeout_ms: Option<u64>,
        #[serde(default)] stdin: Option<String>,
    },

    // Web
    #[serde(rename = "sys_web_fetch")]
    WebFetch { url: String },

    // Memory
    #[serde(rename = "sys_memory_read")]
    MemoryRead { topic: String },
    #[serde(rename = "sys_memory_write")]
    MemoryWrite { topic: String, content: String },
    #[serde(rename = "sys_memory_list")]
    MemoryList {},
}

impl Syscall {
    pub fn method(&self) -> &'static str {
        use Syscall::*;
        match self {
            Reply { .. } => "sys_reply",
            Done {} => "sys_done",
            ReadFile { .. } => "sys_read_file",
            WriteFile { .. } => "sys_write_file",
            ReplaceInFile { .. } => "sys_replace_in_file",
            ListDir { .. } => "sys_list_dir",
            Search { .. } => "sys_search",
            RunCommand { .. } => "sys_run_command",
            WebFetch { .. } => "sys_web_fetch",
            MemoryRead { .. } => "sys_memory_read",
            MemoryWrite { .. } => "sys_memory_write",
            MemoryList {} => "sys_memory_list",
        }
    }

    pub fn all_methods() -> &'static [&'static str] {
        &[
            "sys_reply", "sys_done",
            "sys_read_file", "sys_write_file", "sys_replace_in_file", "sys_list_dir", "sys_search",
            "sys_run_command",
            "sys_web_fetch",
            "sys_memory_read", "sys_memory_write", "sys_memory_list",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_reply() {
        let s = Syscall::Reply { text: "hi".into() };
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["method"], "sys_reply");
        let back: Syscall = serde_json::from_value(j).unwrap();
        assert!(matches!(back, Syscall::Reply { .. }));
    }

    #[test]
    fn all_methods_matches_enum() {
        // Sanity: hand-maintained list matches the actual variants
        assert_eq!(Syscall::all_methods().len(), 12);
    }
}
```

- [ ] **Step 2: Add `schemars` to dependencies (already in workspace) — confirm `arnoldd/Cargo.toml` has it**

In `arnoldd/Cargo.toml` `[dependencies]`:

```toml
schemars.workspace = true
```

- [ ] **Step 3: Write the schema generator**

Create `arnoldd/src/schema.rs`:

```rust
use anyhow::Result;
use schemars::schema_for;
use serde_json::{json, Value};
use crate::syscall::Syscall;

/// Build a constraint_schema for the plan frame, restricted to `allowed_methods`.
/// Output shape matches 439's `schema_for_methods` — a JSON Schema with a `oneOf`
/// containing one branch per method.
pub fn schema_for_methods(allowed_methods: &[&str]) -> Result<Value> {
    let full = serde_json::to_value(schema_for!(Syscall))?;
    let mut branches = vec![];
    // schemars' tagged-content output for our enum is `oneOf` over variant objects.
    if let Some(one_of) = full.pointer("/oneOf").and_then(|v| v.as_array()) {
        for branch in one_of {
            let method_const = branch.pointer("/properties/method/const")
                .or_else(|| branch.pointer("/properties/method/enum/0"))
                .and_then(|v| v.as_str());
            if let Some(m) = method_const {
                if allowed_methods.contains(&m) {
                    branches.push(strip_bare_objects(branch.clone()));
                }
            }
        }
    }
    Ok(json!({ "oneOf": branches }))
}

/// Replace any bare `{}` (which some LLM providers reject) with a permissive object
/// shape. Mirrors 439's normalize_provider_schema().
fn strip_bare_objects(v: Value) -> Value {
    match v {
        Value::Object(mut map) => {
            for (_, val) in map.iter_mut() {
                *val = strip_bare_objects(val.take());
            }
            Value::Object(map)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(strip_bare_objects).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_schema_with_only_allowed_methods() {
        let schema = schema_for_methods(&["sys_reply", "sys_done"]).unwrap();
        let one_of = schema["oneOf"].as_array().unwrap();
        assert_eq!(one_of.len(), 2);
        let methods: Vec<&str> = one_of.iter()
            .filter_map(|b| b.pointer("/properties/method/const").and_then(|v| v.as_str())
                .or_else(|| b.pointer("/properties/method/enum/0").and_then(|v| v.as_str())))
            .collect();
        assert!(methods.contains(&"sys_reply"));
        assert!(methods.contains(&"sys_done"));
    }
}
```

- [ ] **Step 4: Add modules to `main.rs`**

```rust
mod syscall;
mod schema;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p arnoldd syscall && cargo test -p arnoldd schema`

Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add arnoldd/src/syscall.rs arnoldd/src/schema.rs arnoldd/src/main.rs arnoldd/Cargo.toml
git commit -m "feat(arnoldd): typed Syscall enum + constraint_schema generator for cpu plan frame"
```

---

## Task 10: Arnold's `ROLE.md` + plan-frame builder

**Files:**
- Create: `role/ROLE.md`
- Create: `arnoldd/src/plan_frame.rs`
- Modify: `arnoldd/src/main.rs`

This is where Arnold's personality and operating contract for the LLM lives.

- [ ] **Step 1: Author `role/ROLE.md`**

```markdown
---
name: assistant
description: Arnold, a long-running code-focused personal agent
allowed_syscalls:
  - sys_reply
  - sys_done
  - sys_read_file
  - sys_write_file
  - sys_replace_in_file
  - sys_list_dir
  - sys_search
  - sys_run_command
  - sys_web_fetch
  - sys_memory_read
  - sys_memory_write
  - sys_memory_list
---

You are Arnold, a long-running personal coding agent. You live as a daemon on the user's machine. The user talks to you through a CLI client; you persist across sessions; your home is `~/.arnold/`.

## What you do

You help the user with code: reading and editing their projects, running commands, searching the web for docs, and remembering things between sessions so they don't have to re-explain.

## How you act

- Be terse. Short answers, no preamble.
- One syscall per turn. Use `sys_reply` to talk to the user; use `sys_done` to end your turn and wait for what they say next.
- Trust the user. They are a skilled developer; they want results, not warnings.
- When the user asks you to remember something durable (preferences, project facts, decisions), write it to memory with `sys_memory_write`. The taxonomy is `user` / `feedback` / `project` / `reference`.
- When the user asks you to do something on a file, use the project-relative path they gave you. Do not invent paths.

## What's not in this version (v0a)

- You cannot spin up 439 build environments yet. That lands in v0b.
- You cannot schedule future work, cancel in-flight work, or send OS notifications. Those land in v0b/v0.1.
- You have no calendar, mail, or browser tools. Future versions.

## Memory you should always read at session start

`sys_memory_list` to see what's known; `sys_memory_read` for any topic that looks relevant to the user's current request.

## When you make mistakes

Acknowledge them in `sys_reply`. Do not blame the framework or the user. If a syscall errors, read the error message and try a different approach.
```

- [ ] **Step 2: Write `arnoldd/src/plan_frame.rs`**

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use crate::syscall::Syscall;
use crate::schema::schema_for_methods;

const ROLE_MD: &str = include_str!("../../role/ROLE.md");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallSpec {
    pub method: String,
    pub description: String,
    pub example: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolePayload {
    pub name: String,
    pub path: String,
    pub body: String,
    pub allowed_syscalls: Vec<String>,
    pub examples: String,
}

pub struct PlanFrameBuilder {
    pub task_id: Uuid,
    pub context: String,
    pub model_provider: String,
    pub model: String,
}

impl PlanFrameBuilder {
    pub fn build(&self) -> Result<Value> {
        let methods: Vec<&str> = Syscall::all_methods().to_vec();
        let methods_owned: Vec<String> = methods.iter().map(|s| s.to_string()).collect();
        let syscalls: Vec<SyscallSpec> = arnold_syscall_menu();
        let role_payload = RolePayload {
            name: "assistant".into(),
            path: "role/ROLE.md".into(),
            body: ROLE_MD.to_string(),
            allowed_syscalls: methods_owned.clone(),
            examples: String::new(), // No worked examples for v0a
        };
        let constraint_schema = schema_for_methods(&methods)?;
        Ok(json!({
            "type": "plan",
            "task_id": self.task_id.to_string(),
            "task_type": "chat",
            "role": "task",
            "context": self.context,
            "syscalls": syscalls,
            "role_payloads": [role_payload],
            "constraint_schema": constraint_schema,
            "reasoning_controls": {},
            "worker_id": "self",
            "parent_worker_id": null,
            "parent_task_id": null,
            "l2_controls": {},
            "live_tokens_estimate_first_turn": 0,
            "kernel_prompt": "",
            "tool_priority": { "order": [], "low_priority": [] }
        }))
    }
}

fn arnold_syscall_menu() -> Vec<SyscallSpec> {
    let mk = |m: &str, d: &str, e: Value| SyscallSpec {
        method: m.into(), description: d.into(),
        example: serde_json::to_string(&e).unwrap(),
    };
    vec![
        mk("sys_reply", "Send a user-facing text reply.",
            json!({"method":"sys_reply","params":{"text":"Hi"}})),
        mk("sys_done", "End the current turn and wait for the next inbox event.",
            json!({"method":"sys_done","params":{}})),
        mk("sys_read_file", "Read a file relative to a jail root.",
            json!({"method":"sys_read_file","params":{"path":"src/main.rs"}})),
        mk("sys_write_file", "Write a file (creates parent dirs).",
            json!({"method":"sys_write_file","params":{"path":"notes.md","contents":"..."}})),
        mk("sys_replace_in_file", "Find/replace a unique string in a file.",
            json!({"method":"sys_replace_in_file","params":{"path":"src/main.rs","old_string":"foo","new_string":"bar"}})),
        mk("sys_list_dir", "List entries in a directory.",
            json!({"method":"sys_list_dir","params":{"path":"src"}})),
        mk("sys_search", "Ripgrep-style search.",
            json!({"method":"sys_search","params":{"query":"TODO","path":"src","glob":"*.rs"}})),
        mk("sys_run_command", "Run a bounded command and capture output.",
            json!({"method":"sys_run_command","params":{"cmd":"cargo","args":["check"],"timeout_ms":60000}})),
        mk("sys_web_fetch", "Fetch a URL and return its body.",
            json!({"method":"sys_web_fetch","params":{"url":"https://example.com"}})),
        mk("sys_memory_read", "Read a memory topic file.",
            json!({"method":"sys_memory_read","params":{"topic":"user_preferences"}})),
        mk("sys_memory_write", "Create/update a memory topic.",
            json!({"method":"sys_memory_write","params":{"topic":"user_preferences","content":"---\nname: User Preferences\ndescription: ...\ntype: user\n---\nbody"}})),
        mk("sys_memory_list", "Return the MEMORY.md index.",
            json!({"method":"sys_memory_list","params":{}})),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_frame_includes_all_required_top_level_fields() {
        let b = PlanFrameBuilder {
            task_id: Uuid::nil(),
            context: "test".into(),
            model_provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
        };
        let frame = b.build().unwrap();
        for field in ["type","task_id","task_type","role","context","syscalls",
                      "role_payloads","constraint_schema","worker_id","kernel_prompt","tool_priority"] {
            assert!(frame.get(field).is_some(), "missing field: {field}");
        }
        assert_eq!(frame["role"], "task");
        assert_eq!(frame["task_type"], "chat");
        assert_eq!(frame["syscalls"].as_array().unwrap().len(), 12);
    }
}
```

- [ ] **Step 3: Register module**

In `main.rs`:

```rust
mod plan_frame;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p arnoldd plan_frame`

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add role/ROLE.md arnoldd/src/plan_frame.rs arnoldd/src/main.rs
git commit -m "feat(role): author Arnold ROLE.md and plan_frame builder for cpu integration"
```

---

## Task 11: CPU subprocess integration (frame loop)

**Files:**
- Create: `arnoldd/src/cpu_process.rs`
- Modify: `arnoldd/src/main.rs`

Spawns 439's `cpu` binary, writes the plan frame, then drives the step/syscall/result/finished loop. Wires to a typed callback (the syscall handler from Task 12) for actually executing syscalls.

- [ ] **Step 1: Write the cpu_process module**

```rust
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpFrame {
    Syscall { id: u64, method: String, params: Value },
    Finished,
    /// Catch-all for usage/provider_error/archive_l2 etc. that v0a ignores.
    #[serde(other)]
    Other,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownFrame<'a> {
    Plan(Value),
    Step { task_id: &'a str },
    Result { id: u64, result: Value },
    Error { id: u64, message: String },
}

pub struct CpuProcess {
    child: Child,
    stdin: ChildStdin,
    stdout_lines: tokio::io::Lines<BufReader<ChildStdout>>,
    task_id: String,
}

impl CpuProcess {
    pub async fn spawn(
        binary: &Path,
        provider: &str,
        model: &str,
        task_id: String,
    ) -> Result<Self> {
        let mut child = Command::new(binary)
            .env("AGENT_LLM_PROVIDER", provider)
            .env("AGENT_MODEL", model)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| anyhow!("failed to spawn cpu at {}: {e}", binary.display()))?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin on cpu"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout on cpu"))?;
        let stdout_lines = BufReader::new(stdout).lines();
        Ok(Self { child, stdin, stdout_lines, task_id })
    }

    pub async fn send_plan(&mut self, plan: Value) -> Result<()> {
        self.write_frame(&DownFrame::Plan(plan)).await
    }

    pub async fn send_step(&mut self) -> Result<()> {
        let frame = DownFrame::Step { task_id: &self.task_id };
        self.write_frame(&frame).await
    }

    pub async fn send_result(&mut self, id: u64, result: Value) -> Result<()> {
        self.write_frame(&DownFrame::Result { id, result }).await
    }

    pub async fn send_error(&mut self, id: u64, message: String) -> Result<()> {
        self.write_frame(&DownFrame::Error { id, message }).await
    }

    pub async fn read_up(&mut self) -> Result<Option<UpFrame>> {
        loop {
            let Some(line) = self.stdout_lines.next_line().await? else { return Ok(None); };
            if line.trim().is_empty() { continue; }
            let frame: UpFrame = serde_json::from_str(&line)
                .map_err(|e| anyhow!("cpu sent malformed frame '{line}': {e}"))?;
            return Ok(Some(frame));
        }
    }

    async fn write_frame<T: serde::Serialize>(&mut self, frame: &T) -> Result<()> {
        let mut bytes = serde_json::to_vec(frame)?;
        bytes.push(b'\n');
        self.stdin.write_all(&bytes).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    pub async fn shutdown(mut self) -> Result<()> {
        drop(self.stdin);
        let _ = self.child.kill().await;
        Ok(())
    }
}
```

- [ ] **Step 2: Register module + add a smoke test that does NOT spawn cpu (since cpu may not exist at test time)**

In `main.rs`:

```rust
mod cpu_process;
```

The cpu integration is end-to-end tested in Task 16 (smoke test) where the cpu binary is guaranteed built.

- [ ] **Step 3: Verify compile**

Run: `cargo check -p arnoldd`

Expected: clean compile.

- [ ] **Step 4: Commit**

```bash
git add arnoldd/src/cpu_process.rs arnoldd/src/main.rs
git commit -m "feat(arnoldd): cpu subprocess + JSON-line frame loop (plan/step/result/error ↔ syscall/finished)"
```

---

## Task 12: Syscall handler trait + dispatch

**Files:**
- Create: `arnoldd/src/handler.rs`
- Create: `arnoldd/src/handlers/mod.rs`
- Modify: `arnoldd/src/main.rs`

The dispatch layer that takes a parsed `Syscall` and routes to the right handler family, returning a `serde_json::Value` (the syscall result for `cpu`).

- [ ] **Step 1: Write the dispatcher**

Create `arnoldd/src/handler.rs`:

```rust
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use crate::syscall::Syscall;
use crate::jail::Jail;
use crate::memory::MemoryStore;
use crate::session::SessionState;

pub struct HandlerContext {
    pub jail: Arc<Jail>,
    pub memory: Arc<MemoryStore>,
    pub session: SessionState,
}

pub async fn dispatch(ctx: &HandlerContext, syscall: Syscall) -> Result<Value> {
    use Syscall::*;
    match syscall {
        Reply { text } => crate::handlers::lifecycle::reply(ctx, text).await,
        Done {} => crate::handlers::lifecycle::done(ctx).await,
        ReadFile { path } => crate::handlers::file::read_file(ctx, path).await,
        WriteFile { path, contents } => crate::handlers::file::write_file(ctx, path, contents).await,
        ReplaceInFile { path, old_string, new_string } =>
            crate::handlers::file::replace_in_file(ctx, path, old_string, new_string).await,
        ListDir { path } => crate::handlers::file::list_dir(ctx, path).await,
        Search { query, path, glob } => crate::handlers::file::search(ctx, query, path, glob).await,
        RunCommand { cmd, args, cwd, timeout_ms, stdin } =>
            crate::handlers::shell::run_command(ctx, cmd, args, cwd, timeout_ms, stdin).await,
        WebFetch { url } => crate::handlers::web::web_fetch(ctx, url).await,
        MemoryRead { topic } => crate::handlers::memory_syscalls::memory_read(ctx, topic).await,
        MemoryWrite { topic, content } => crate::handlers::memory_syscalls::memory_write(ctx, topic, content).await,
        MemoryList {} => crate::handlers::memory_syscalls::memory_list(ctx).await,
    }
}
```

- [ ] **Step 2: Create empty handler module skeletons**

Create `arnoldd/src/handlers/mod.rs`:

```rust
pub mod lifecycle;
pub mod file;
pub mod shell;
pub mod web;
pub mod memory_syscalls;
```

Create empty placeholder files (each gets filled in Tasks 13–15):

```bash
mkdir -p arnoldd/src/handlers
touch arnoldd/src/handlers/lifecycle.rs
touch arnoldd/src/handlers/file.rs
touch arnoldd/src/handlers/shell.rs
touch arnoldd/src/handlers/web.rs
touch arnoldd/src/handlers/memory_syscalls.rs
```

Write a placeholder stub in each that returns `unimplemented!()` so Tasks 13–15 can replace cleanly. Example for `lifecycle.rs`:

```rust
use anyhow::Result;
use serde_json::Value;
use crate::handler::HandlerContext;

pub async fn reply(_ctx: &HandlerContext, _text: String) -> Result<Value> { unimplemented!("Task 13") }
pub async fn done(_ctx: &HandlerContext) -> Result<Value> { unimplemented!("Task 13") }
```

Same pattern for the other four files — declare every function dispatch.rs imports, body = `unimplemented!`. (See Task 13/14/15 for the exact signatures.)

- [ ] **Step 3: Register handler module**

In `main.rs`:

```rust
mod handler;
mod handlers;
```

- [ ] **Step 4: Verify compile**

Run: `cargo check -p arnoldd`

Expected: clean compile (the `unimplemented!()` stubs satisfy the dispatcher's type signature).

- [ ] **Step 5: Commit**

```bash
git add arnoldd/src/handler.rs arnoldd/src/handlers/ arnoldd/src/main.rs
git commit -m "feat(arnoldd): syscall handler trait + dispatch skeleton (handlers stubbed)"
```

---

## Task 13: Conversation + file handlers

**Files:**
- Modify: `arnoldd/src/handlers/lifecycle.rs`
- Modify: `arnoldd/src/handlers/file.rs`

- [ ] **Step 1: Implement `handlers/lifecycle.rs`**

```rust
use anyhow::Result;
use arnold_wire::DaemonEvent;
use serde_json::{json, Value};
use crate::handler::HandlerContext;

pub async fn reply(ctx: &HandlerContext, text: String) -> Result<Value> {
    let _ = ctx.session.client.send(DaemonEvent::Reply {
        session_id: ctx.session.session_id,
        text: text.clone(),
    });
    Ok(json!({"status": "delivered"}))
}

pub async fn done(ctx: &HandlerContext) -> Result<Value> {
    let _ = ctx.session.client.send(DaemonEvent::TurnComplete {
        session_id: ctx.session.session_id,
    });
    Ok(json!({"status": "turn_complete"}))
}
```

- [ ] **Step 2: Implement `handlers/file.rs`**

```rust
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use crate::handler::HandlerContext;

pub async fn read_file(ctx: &HandlerContext, path: String) -> Result<Value> {
    let abs = ctx.jail.resolve(&path)?;
    let contents = tokio::fs::read_to_string(&abs).await?;
    // Truncate to keep CPU context bounded
    let truncated = contents.chars().take(8000).collect::<String>();
    Ok(json!({"contents": truncated, "truncated": contents.len() > 8000}))
}

pub async fn write_file(ctx: &HandlerContext, path: String, contents: String) -> Result<Value> {
    let abs = ctx.jail.resolve(&path)?;
    if let Some(parent) = abs.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&abs, &contents).await?;
    Ok(json!({"bytes_written": contents.len()}))
}

pub async fn replace_in_file(
    ctx: &HandlerContext,
    path: String,
    old_string: String,
    new_string: String,
) -> Result<Value> {
    let abs = ctx.jail.resolve(&path)?;
    let original = tokio::fs::read_to_string(&abs).await?;
    let occurrences = original.matches(&old_string).count();
    if occurrences == 0 {
        return Err(anyhow!("old_string not found in file"));
    }
    if occurrences > 1 {
        return Err(anyhow!("old_string matched {occurrences} times; must be unique"));
    }
    let updated = original.replacen(&old_string, &new_string, 1);
    tokio::fs::write(&abs, &updated).await?;
    Ok(json!({"replaced": 1}))
}

pub async fn list_dir(ctx: &HandlerContext, path: String) -> Result<Value> {
    let abs = ctx.jail.resolve(&path)?;
    let mut entries = vec![];
    let mut rd = tokio::fs::read_dir(&abs).await?;
    while let Some(e) = rd.next_entry().await? {
        let ft = e.file_type().await?;
        entries.push(json!({
            "name": e.file_name().to_string_lossy(),
            "is_dir": ft.is_dir(),
        }));
    }
    Ok(json!({"entries": entries}))
}

pub async fn search(
    ctx: &HandlerContext,
    query: String,
    path: Option<String>,
    glob: Option<String>,
) -> Result<Value> {
    let search_path = match path {
        Some(p) => ctx.jail.resolve(&p)?,
        None => ctx.session.cwd.clone(),
    };
    let mut cmd = Command::new("rg");
    cmd.arg("--json").arg("--max-count").arg("20").arg("--max-columns").arg("200");
    if let Some(g) = glob { cmd.arg("--glob").arg(g); }
    cmd.arg(&query).arg(&search_path);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let mut out = String::new();
    if let Some(mut so) = child.stdout.take() {
        so.read_to_string(&mut out).await?;
    }
    let _ = child.wait().await?;
    // Parse one line per match
    let mut matches = vec![];
    for line in out.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        if v["type"] == "match" {
            matches.push(json!({
                "path": v["data"]["path"]["text"],
                "line_number": v["data"]["line_number"],
                "text": v["data"]["lines"]["text"],
            }));
        }
        if matches.len() >= 50 { break; }
    }
    Ok(json!({"matches": matches}))
}
```

- [ ] **Step 3: Write tests for the file handlers (lifecycle handlers are tested end-to-end in Task 16)**

Add at the bottom of `handlers/file.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::jail::Jail;
    use crate::memory::MemoryStore;
    use crate::session::SessionState;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    fn ctx(jail_root: &std::path::Path) -> HandlerContext {
        let (tx, _rx) = mpsc::unbounded_channel();
        HandlerContext {
            jail: Arc::new(Jail::new(vec![jail_root.to_path_buf()])),
            memory: Arc::new(MemoryStore::open(&jail_root.join("mem")).unwrap()),
            session: SessionState {
                session_id: Uuid::nil(),
                cwd: jail_root.to_path_buf(),
                client: tx,
            },
        }
    }

    #[tokio::test]
    async fn write_then_read() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        write_file(&c, "hello.txt".into(), "world".into()).await.unwrap();
        let r = read_file(&c, "hello.txt".into()).await.unwrap();
        assert_eq!(r["contents"], "world");
    }

    #[tokio::test]
    async fn replace_requires_unique_match() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        write_file(&c, "a.txt".into(), "foo bar foo".into()).await.unwrap();
        let err = replace_in_file(&c, "a.txt".into(), "foo".into(), "baz".into()).await;
        assert!(err.is_err());
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p arnoldd handlers::file`

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add arnoldd/src/handlers/lifecycle.rs arnoldd/src/handlers/file.rs
git commit -m "feat(handlers): conversation lifecycle + file handlers (read/write/replace/list/search)"
```

---

## Task 14: Shell + web handlers

**Files:**
- Modify: `arnoldd/src/handlers/shell.rs`
- Modify: `arnoldd/src/handlers/web.rs`

- [ ] **Step 1: Implement `handlers/shell.rs`**

```rust
use anyhow::Result;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use crate::handler::HandlerContext;

pub async fn run_command(
    ctx: &HandlerContext,
    cmd: String,
    args: Vec<String>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    stdin: Option<String>,
) -> Result<Value> {
    let cwd_abs = match cwd {
        Some(p) => ctx.jail.resolve(&p)?,
        None => ctx.session.cwd.clone(),
    };
    let mut child = Command::new(&cmd)
        .args(&args)
        .current_dir(&cwd_abs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(input) = stdin {
        if let Some(mut s) = child.stdin.take() {
            s.write_all(input.as_bytes()).await?;
            drop(s);
        }
    }

    let timeout = Duration::from_millis(timeout_ms.unwrap_or(60_000));
    let out = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(o) => o?,
        Err(_) => {
            // Best-effort kill; child already moved into wait_with_output, so we cannot
            // kill it here directly. The future is dropped, which on stable Tokio cancels
            // the await but leaves the process running until natural exit. v0a accepts this.
            return Err(anyhow::anyhow!("command timed out after {timeout_ms:?} ms"));
        }
    };
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let trunc = |s: String| if s.len() > 8000 { (s.chars().take(8000).collect::<String>(), true) } else { (s, false) };
    let (stdout_t, stdout_truncated) = trunc(stdout);
    let (stderr_t, stderr_truncated) = trunc(stderr);
    Ok(json!({
        "exit_code": out.status.code(),
        "stdout": stdout_t,
        "stderr": stderr_t,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
    }))
}
```

- [ ] **Step 2: Implement `handlers/web.rs`**

```rust
use anyhow::Result;
use serde_json::{json, Value};
use std::time::Duration;
use crate::handler::HandlerContext;

pub async fn web_fetch(_ctx: &HandlerContext, url: String) -> Result<Value> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow::anyhow!("only http(s) URLs are allowed"));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Arnold/0.1 (+https://github.com/jbankse/Arnold)")
        .build()?;
    let resp = client.get(&url).send().await?;
    let status = resp.status().as_u16();
    let body = resp.text().await?;
    let truncated = body.chars().take(16_000).collect::<String>();
    Ok(json!({
        "status": status,
        "body": truncated,
        "body_truncated": body.len() > 16_000,
    }))
}
```

- [ ] **Step 3: Add a smoke test that uses a local HTTP server**

Add at the bottom of `handlers/web.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::jail::Jail;
    use crate::memory::MemoryStore;
    use crate::session::SessionState;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    fn ctx(root: &std::path::Path) -> HandlerContext {
        let (tx, _rx) = mpsc::unbounded_channel();
        HandlerContext {
            jail: Arc::new(Jail::new(vec![root.to_path_buf()])),
            memory: Arc::new(MemoryStore::open(&root.join("mem")).unwrap()),
            session: SessionState { session_id: Uuid::nil(), cwd: root.to_path_buf(), client: tx },
        }
    }

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        assert!(web_fetch(&c, "file:///etc/passwd".into()).await.is_err());
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p arnoldd handlers::web && cargo test -p arnoldd handlers::shell`

Expected: 1 passed (web). Shell handler is exercised in the smoke test.

- [ ] **Step 5: Commit**

```bash
git add arnoldd/src/handlers/shell.rs arnoldd/src/handlers/web.rs
git commit -m "feat(handlers): shell run_command + web_fetch (http(s) only, bounded body)"
```

---

## Task 15: Memory handlers

**Files:**
- Modify: `arnoldd/src/handlers/memory_syscalls.rs`

- [ ] **Step 1: Implement `handlers/memory_syscalls.rs`**

```rust
use anyhow::Result;
use serde_json::{json, Value};
use crate::handler::HandlerContext;
use crate::memory::MemoryTopic;

pub async fn memory_read(ctx: &HandlerContext, topic: String) -> Result<Value> {
    let t = ctx.memory.read(&topic)?;
    Ok(json!({
        "topic": topic,
        "name": t.name,
        "description": t.description,
        "type": serde_json::to_value(&t.kind)?,
        "body": t.body,
    }))
}

pub async fn memory_write(ctx: &HandlerContext, topic: String, content: String) -> Result<Value> {
    // The model passes the full file body (frontmatter + body). Parse it to validate.
    let parsed: MemoryTopic = parse_full_topic_file(&content)?;
    ctx.memory.write(&topic, &parsed)?;
    Ok(json!({"status": "written"}))
}

pub async fn memory_list(ctx: &HandlerContext) -> Result<Value> {
    Ok(json!({"index": ctx.memory.index()?}))
}

fn parse_full_topic_file(text: &str) -> Result<MemoryTopic> {
    let stripped = text.strip_prefix("---\n")
        .ok_or_else(|| anyhow::anyhow!("missing frontmatter; expected ---\\n"))?;
    let end = stripped.find("\n---\n")
        .ok_or_else(|| anyhow::anyhow!("unterminated frontmatter; expected closing \\n---\\n"))?;
    let frontmatter = &stripped[..end];
    let body = &stripped[end + 5..];
    #[derive(serde::Deserialize)]
    struct Fm { name: String, description: String, #[serde(rename = "type")] kind: crate::memory::MemoryType }
    let fm: Fm = serde_yaml::from_str(frontmatter)?;
    Ok(MemoryTopic { name: fm.name, description: fm.description, kind: fm.kind, body: body.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jail::Jail;
    use crate::memory::MemoryStore;
    use crate::session::SessionState;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    fn ctx(root: &std::path::Path) -> HandlerContext {
        let (tx, _rx) = mpsc::unbounded_channel();
        HandlerContext {
            jail: Arc::new(Jail::new(vec![root.to_path_buf()])),
            memory: Arc::new(MemoryStore::open(&root.join("mem")).unwrap()),
            session: SessionState { session_id: Uuid::nil(), cwd: root.to_path_buf(), client: tx },
        }
    }

    #[tokio::test]
    async fn write_then_read_then_list() {
        let tmp = TempDir::new().unwrap();
        let c = ctx(tmp.path());
        let body = "---\nname: Test\ndescription: a test\ntype: user\n---\nbody text\n";
        memory_write(&c, "test".into(), body.into()).await.unwrap();
        let r = memory_read(&c, "test".into()).await.unwrap();
        assert_eq!(r["name"], "Test");
        let l = memory_list(&c).await.unwrap();
        assert!(l["index"].as_str().unwrap().contains("Test"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p arnoldd handlers::memory_syscalls`

Expected: 1 passed.

- [ ] **Step 3: Commit**

```bash
git add arnoldd/src/handlers/memory_syscalls.rs
git commit -m "feat(handlers): memory_read / memory_write / memory_list with frontmatter validation"
```

---

## Task 16: Wire session loop to real cpu integration

**Files:**
- Modify: `arnoldd/src/session.rs` (replace stub with real cpu turn loop)
- Modify: `arnoldd/src/main.rs` (build jail + memory, pass to sessions)

This is the integration moment: the stub `handle_user_message_stub` from Task 6 gets replaced with a real loop that spawns `cpu`, sends the plan frame, drives step/syscall/result, dispatches via Task 12's handler, and exits on `Finished`.

- [ ] **Step 1: Rewrite `session.rs` with the real turn handler**

```rust
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
        let frame = cpu.read_up().await?;
        match frame {
            None | Some(UpFrame::Finished) => break,
            Some(UpFrame::Other) => continue,
            Some(UpFrame::Syscall { id, method, params }) => {
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
```

- [ ] **Step 2: Update `uds_server.rs` to use `DaemonState` instead of separate `SessionRegistry` + `Inbox`**

Replace `handle_connection` and `route_request` in `uds_server.rs`:

```rust
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
```

- [ ] **Step 3: Update `main.rs` to construct `DaemonState`**

```rust
mod config;
mod cpu_process;
mod handler;
mod handlers;
mod inbox;
mod jail;
mod memory;
mod plan_frame;
mod schema;
mod session;
mod syscall;
mod uds_server;

use anyhow::Result;
use tracing::{info, error};
use config::ArnoldConfig;
use inbox::Inbox;
use jail::Jail;
use memory::MemoryStore;
use session::DaemonState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("arnoldd=info".parse()?))
        .init();

    let config = ArnoldConfig::load_or_default()?;
    let arnold_dir = ArnoldConfig::arnold_dir()?;
    let sock_path = ArnoldConfig::socket_path()?;
    let inbox = Inbox::open(&arnold_dir.join("inbox.db"))?;
    let memory = MemoryStore::open(&arnold_dir.join("memory"))?;
    let mut jail_roots = vec![arnold_dir.join("workspace")];
    jail_roots.extend(config.allowed_dirs.iter().cloned());
    std::fs::create_dir_all(&jail_roots[0])?;
    let jail = Jail::new(jail_roots);

    let state = DaemonState::new(config, jail, memory);

    info!(socket = %sock_path.display(), "arnoldd starting");
    let listener = uds_server::bind(&sock_path).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let s = state.clone();
        let i = inbox.clone();
        tokio::spawn(async move {
            if let Err(e) = uds_server::handle_connection(stream, s, i).await {
                error!("connection handler error: {e:#}");
            }
        });
    }
}
```

- [ ] **Step 4: Verify the whole workspace compiles**

Run: `cargo check --workspace`

Expected: clean.

- [ ] **Step 5: Run all tests**

Run: `cargo test --workspace`

Expected: all passing.

- [ ] **Step 6: Commit**

```bash
git add arnoldd/src/session.rs arnoldd/src/uds_server.rs arnoldd/src/main.rs
git commit -m "feat(arnoldd): wire real cpu turn loop — handler dispatch + syscall reply path"
```

---

## Task 17: Install script + end-to-end smoke test

**Files:**
- Create: `scripts/install.sh`
- Create: `scripts/uninstall.sh`
- Create: `arnoldd/tests/smoke_e2e.rs` (optional automated smoke; manual instructions also given)

- [ ] **Step 1: Write `scripts/install.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"
ARNOLD_DIR="${ARNOLD_DIR:-$HOME/.arnold}"

echo "[1/4] building 439 cpu binary..."
"$HERE/build-cpu.sh" "$ARNOLD_DIR/bin"

echo "[2/4] building arnoldd + arnold..."
cd "$ROOT"
cargo build --release -p arnoldd -p arnold-cli

echo "[3/4] installing binaries..."
mkdir -p "$BIN_DIR" "$ARNOLD_DIR/workspace" "$ARNOLD_DIR/memory" "$ARNOLD_DIR/jobs"
install -m 755 "$ROOT/target/release/arnoldd" "$BIN_DIR/arnoldd"
install -m 755 "$ROOT/target/release/arnold" "$BIN_DIR/arnold"

echo "[4/4] writing default config (if absent)..."
if [ ! -f "$ARNOLD_DIR/config.toml" ]; then
  cat > "$ARNOLD_DIR/config.toml" <<EOF
cpu_binary = "$ARNOLD_DIR/bin/cpu"
llm_provider = "anthropic"
model = "claude-sonnet-4-6"
allowed_dirs = []
EOF
fi

echo
echo "Installed. Start arnoldd manually for v0a:"
echo "  $BIN_DIR/arnoldd"
echo "Then in another terminal:"
echo "  $BIN_DIR/arnold"
echo
echo "launchd/systemd unit installation lands in v0b."
```

```bash
chmod +x scripts/install.sh
```

- [ ] **Step 2: Write `scripts/uninstall.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"
ARNOLD_DIR="${ARNOLD_DIR:-$HOME/.arnold}"
rm -f "$BIN_DIR/arnoldd" "$BIN_DIR/arnold"
echo "Removed binaries. To remove state, manually delete $ARNOLD_DIR"
```

```bash
chmod +x scripts/uninstall.sh
```

- [ ] **Step 3: Run the install script**

```bash
./scripts/install.sh
```

Expected: clean exit, `~/.arnold/bin/cpu`, `~/.local/bin/arnoldd`, `~/.local/bin/arnold` all exist.

- [ ] **Step 4: Run the manual end-to-end smoke test**

Set `ANTHROPIC_API_KEY` (or whichever provider's key the config points at), then:

```bash
~/.local/bin/arnoldd &
ARNOLDD=$!
sleep 1
~/.local/bin/arnold
# In the REPL, type: "Reply with the word OK and end your turn."
# Expect: arnold: OK   (and turn complete)
# Ctrl-D to exit
kill $ARNOLDD
```

Expected: Arnold replies "OK" (or close to it), `TurnComplete` prints, Ctrl-D exits cleanly, daemon shut down via kill.

- [ ] **Step 5: Add a brief HOWTO note to the README**

Append to `README.md`:

```markdown

## Quickstart (v0a)

```bash
./scripts/install.sh
export ANTHROPIC_API_KEY=sk-...
~/.local/bin/arnoldd &
~/.local/bin/arnold
> Hello Arnold, what can you do?
```
```

- [ ] **Step 6: Commit**

```bash
git add scripts/install.sh scripts/uninstall.sh README.md
git commit -m "feat: install/uninstall scripts + v0a quickstart in README"
```

---

## Done. v0a ships.

After Task 17, `arnoldd` + `arnold` + the cpu binary together give the user a working Claude-Code-style assistant living in their terminal, persisting memory across sessions, with all the lower-half infrastructure that v0b needs (`sys_spin_up_439`, `sys_send_notification`, Go TUI client, launchd/systemd unit, in-flight job table) ready to slot into the existing structure without rework.

v0b plan is written separately after v0a is shipped and we know its concrete interfaces.
