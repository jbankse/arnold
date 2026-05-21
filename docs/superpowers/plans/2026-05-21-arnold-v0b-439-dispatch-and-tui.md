# Arnold v0b — 439 Dispatch, Go TUI, Tray, and Autostart Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land Arnold's headline `sys_spin_up_439` async syscall + in-flight job table + BIOS subprocess + completion-event inbox delivery, plus retire the interim Rust REPL by shipping a Go bubbletea TUI (`arnold`) and a macOS menu bar status icon (`arnold-tray`), wired to launchd/systemd user services so the daemon and tray autostart at login.

**Architecture:** The Rust daemon (`arnoldd`) gains four new subsystems — an `Inbox`-adjacent SQLite job table, a `BiosProcess` driver that spawns `bios run` and turns its `RunOutcome` into `JobCompleted` inbox events, a usage-frame interceptor that turns cpu's `usage` up-frames into per-session cost telemetry, and a notification dispatcher (osascript on macOS, notify-send on Linux). The interim `arnold-cli` Rust REPL is replaced wholesale by a Go binary `arnold` (bubbletea + bubbles) with four panes — conversation, inbox, jobs, status bar with cost meter — communicating with the daemon over the same UDS + JSON-line wire. A small sibling Go binary `arnold-tray` (getlantern/systray) lives in the menu bar showing daemon status with an "Open arnold" quick-launch. macOS LaunchAgent plists and Linux systemd user units autostart both binaries at login.

**Tech Stack:**
- **Rust 1.75+** for daemon additions, building on the v0a Cargo workspace; new deps: `notify-rust` (cross-platform notifications fallback for sys_send_notification's structured form), `which` (locate `bios` binary at startup).
- **Go 1.21+** for the TUI and tray, new top-level modules `arnold-tui/` and `arnold-tray/` outside the Cargo workspace; deps: `charmbracelet/bubbletea`, `charmbracelet/bubbles`, `charmbracelet/lipgloss` (TUI styling), `getlantern/systray` (tray, macOS-only in v0b), plus stdlib `encoding/json`, `net` (UDS).
- **439 submodule** consumed at the same pinned tag as v0a; `bios` binary built into `~/.arnold/bin/bios` by the install script alongside `cpu`.
- **launchd** on macOS via `~/Library/LaunchAgents/com.arnold.arnoldd.plist` and `com.arnold.arnold-tray.plist`; **systemd user** on Linux via `~/.config/systemd/user/arnoldd.service` (no tray on Linux in v0b — defer).

---

## What v0b Pulls In Beyond v0a Spec

In addition to the spec's v0b scope, this plan folds in the two Opus-flagged deferrals from v0a's final review:

- **`arnoldd --check` pre-flight mode** — spawn cpu with a minimal no-op plan, expect a `finished` frame, exit cleanly. Used by the launchd unit's `RunAtLoad` health check and by Joshua when debugging.
- **Friendlier `rg` missing diagnostic** in `handlers/file.rs::search` — wrap the spawn error with "ripgrep (rg) not found on PATH" guidance.

It also picks up one direction that came out of Joshua's session with the v0b plan author:

- **Menu bar icon (`arnold-tray`)** — separate Go binary using `getlantern/systray`, macOS-only in v0b. Minimal scope: status dot (green=running / red=stopped / yellow=connecting), menu items "Open arnold", "Restart daemon", "Quit". Notification badge + cost-in-tray defer to v0.1.

---

## File Structure

v0b adds two new top-level Go modules and several new Rust source files inside the existing `arnoldd/` crate. The Rust workspace stays a single Cargo workspace (Go modules are independent of Cargo).

```
Codebases/Arnold/
├── Cargo.toml                              # MODIFY: remove `arnold-cli` from workspace members
├── arnold-wire/src/lib.rs                  # MODIFY: add JobSpawned/JobUpdate/JobCompleted, CostUpdate, NotificationDispatched events
│
├── arnoldd/
│   ├── Cargo.toml                          # MODIFY: add `notify-rust`, `which` deps
│   └── src/
│       ├── main.rs                         # MODIFY: register new modules; add `--check` flag handling
│       ├── jobs.rs                         # NEW: in-flight job table (SQLite-backed, share inbox.db connection)
│       ├── bios_process.rs                 # NEW: spawn `bios run`, parse RunOutcome, emit JobCompleted via inbox
│       ├── notify.rs                       # NEW: OS notification dispatcher (osascript on macOS, notify-send on Linux fallback)
│       ├── usage_meter.rs                  # NEW: intercept cpu `usage` frames, accumulate session cost, emit CostUpdate
│       ├── precheck.rs                     # NEW: `--check` mode (spawn cpu, send minimal plan, expect `finished`)
│       ├── session.rs                      # MODIFY: integrate usage_meter; handle background JobCompleted delivery; route new syscalls
│       ├── cpu_process.rs                  # MODIFY: change read_up to return UpFrame::Usage variant explicitly (not Other) so usage_meter can consume it
│       ├── syscall.rs                      # MODIFY: add SpinUp439, PollJob, SendNotification variants to Syscall enum
│       ├── schema.rs                       # unchanged (the schema generator picks up new variants automatically)
│       ├── plan_frame.rs                   # MODIFY: extend arnold_syscall_menu() with 3 new SyscallSpec entries
│       └── handlers/
│           ├── mod.rs                      # MODIFY: register new handler modules
│           ├── dispatch_439.rs             # NEW: sys_spin_up_439, sys_poll_job
│           ├── notifications.rs            # NEW: sys_send_notification
│           ├── file.rs                     # MODIFY: friendlier rg-missing diagnostic in `search`
│           └── ...existing handlers unchanged...
│
├── arnold-cli/                             # DELETE: entire crate retired in favor of Go TUI
│
├── arnold-tui/                             # NEW: Go bubbletea TUI
│   ├── go.mod                              # `module github.com/jbankse/arnold/arnold-tui` (path-only, no go.sum publishing)
│   ├── go.sum
│   ├── main.go                             # entry: connect, run bubbletea program, clean exit
│   ├── wire/wire.go                        # ClientRequest / DaemonEvent type mirrors of arnold-wire (snake_case JSON)
│   ├── client/client.go                    # UDS connection, send/receive helpers
│   ├── client/cost.go                      # cost-accumulator helper consuming CostUpdate events
│   └── tui/
│       ├── model.go                        # top-level bubbletea Model (root state, pane focus, Update/View)
│       ├── conversation.go                 # conversation pane: scrollable history + input prompt
│       ├── inbox.go                        # inbox pane: list of InboxEvent rows
│       ├── jobs.go                         # jobs pane: list of in-flight jobs with status indicator
│       ├── statusbar.go                    # status bar: provider/model, session cost, daemon connection state, focused pane hint
│       ├── keybindings.go                  # global keybindings (j/k navigate, tab switch pane, i to insert, esc to normal, q to quit, ? help)
│       └── help.go                         # `?`-pressed help overlay
│
├── arnold-tray/                            # NEW: macOS menu bar status icon (Go)
│   ├── go.mod                              # `module github.com/jbankse/arnold/arnold-tray`
│   ├── go.sum
│   ├── main.go                             # systray.Run; ping daemon on interval; build menu items
│   ├── icon.go                             # embedded PNG bytes for green/red/yellow icons via go:embed
│   ├── assets/icon_green.png               # 32x32 green dot (daemon running)
│   ├── assets/icon_red.png                 # 32x32 red dot (daemon down)
│   └── assets/icon_yellow.png              # 32x32 yellow dot (connecting / unknown)
│
├── scripts/
│   ├── build-cpu.sh                        # unchanged from v0a
│   ├── build-bios.sh                       # NEW: cargo-build `bios` from vendor/439, install to ~/.arnold/bin/bios
│   ├── build-tui.sh                        # NEW: `go build` arnold-tui, install to $BIN_DIR/arnold
│   ├── build-tray.sh                       # NEW: `go build` arnold-tray, install to $BIN_DIR/arnold-tray (macOS only)
│   ├── install.sh                          # MODIFY: orchestrate cpu+bios+arnoldd+TUI+tray, install plist/service, enable autostart
│   ├── uninstall.sh                        # MODIFY: stop services, remove plist/service, remove all binaries
│   ├── arnoldd.plist.template              # NEW: macOS LaunchAgent plist for arnoldd
│   ├── arnold-tray.plist.template          # NEW: macOS LaunchAgent plist for arnold-tray
│   └── arnoldd.service.template            # NEW: Linux systemd user unit for arnoldd
│
├── role/ROLE.md                            # MODIFY: add sections describing sys_spin_up_439, sys_poll_job, sys_send_notification
├── README.md                               # MODIFY: replace v0a Quickstart with v0b
├── CLAUDE.md                               # MODIFY: note the Go modules as separate workspaces from the Cargo workspace
└── docs/superpowers/plans/2026-05-21-arnold-v0b-439-dispatch-and-tui.md  # this plan
```

**Rationale for the split:**
- `arnold-tui/` and `arnold-tray/` are sibling Go modules at the repo root (not nested under one parent module) because they have entirely different dep sets and ship as independent binaries — keeping them siblings means each has a focused `go.sum` and one can be deleted without disturbing the other.
- New daemon modules (`jobs.rs`, `bios_process.rs`, `notify.rs`, `usage_meter.rs`, `precheck.rs`) each own one v0b capability so they can be reviewed independently. The handler modules (`dispatch_439.rs`, `notifications.rs`) follow the established v0a pattern of one file per syscall family.
- Three small build scripts (`build-bios.sh`, `build-tui.sh`, `build-tray.sh`) parallel v0a's `build-cpu.sh`. Each is independently runnable for dev iteration; `install.sh` orchestrates them.

---

## Task 1: Extend `arnold-wire` with v0b event variants

**Files:**
- Modify: `arnold-wire/src/lib.rs`

This task adds 5 new `DaemonEvent` variants and 0 new `ClientRequest` variants. New `ClientRequest`s aren't needed in v0b because all v0b client capabilities (spawning a 439 job, polling a job, sending a notification) are LLM-driven syscalls invoked through normal `UserMessage` turns. Client-initiated `ClientRequest`s only model user-facing operations on the daemon itself (open session, send message, close session) — those didn't change.

The 5 new `DaemonEvent`s the daemon pushes:

- `JobSpawned { session_id, job_id, prompt_preview }` — emitted after `sys_spin_up_439` accepts and starts a background BIOS run. The CLI shows "Job <id> started" in the jobs pane.
- `JobUpdate { session_id, job_id, status, last_log_line }` — periodic heartbeat from the BIOS driver so the TUI's jobs pane can show liveness. Status: `queued | running | exporting | completed | failed | cancelled`.
- `JobCompleted { session_id, job_id, status, exit_code, exported_workspace, message }` — terminal event when BIOS finishes. Drives the jobs-pane "completed" row and (via the inbox) becomes a turn-input on the user's next message.
- `CostUpdate { session_id, session_cost_usd, last_turn_cost_usd, last_turn_provider, last_turn_model }` — emitted after each cpu turn that produced a `usage` frame. Drives the TUI status bar's cost meter.
- `Notification { session_id, title, body, urgency }` — Arnold dispatching a notification via `sys_send_notification`. The TUI logs it; OS notification dispatch happens in the daemon's `notify` module.

- [ ] **Step 1: Write the test for round-trip of each new variant**

Append to `arnold-wire/src/lib.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn job_spawned_round_trip() {
        let ev = DaemonEvent::JobSpawned {
            session_id: Uuid::nil(),
            job_id: Uuid::nil(),
            prompt_preview: "make a counter app".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, parsed);
        assert!(json.contains("\"type\":\"job_spawned\""));
    }

    #[test]
    fn job_completed_round_trip() {
        let ev = DaemonEvent::JobCompleted {
            session_id: Uuid::nil(),
            job_id: Uuid::nil(),
            status: "completed".into(),
            exit_code: Some(0),
            exported_workspace: Some("/Users/joshua/.arnold/jobs/abc/workspace".into()),
            message: "runtime completed successfully".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, parsed);
        assert!(json.contains("\"type\":\"job_completed\""));
    }

    #[test]
    fn cost_update_round_trip() {
        let ev = DaemonEvent::CostUpdate {
            session_id: Uuid::nil(),
            session_cost_usd: 0.0123,
            last_turn_cost_usd: 0.0042,
            last_turn_provider: "anthropic".into(),
            last_turn_model: "claude-sonnet-4-6".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, parsed);
        assert!(json.contains("\"type\":\"cost_update\""));
    }

    #[test]
    fn notification_round_trip() {
        let ev = DaemonEvent::Notification {
            session_id: Uuid::nil(),
            title: "Job complete".into(),
            body: "Counter app build succeeded".into(),
            urgency: "normal".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, parsed);
        assert!(json.contains("\"type\":\"notification\""));
    }
```

- [ ] **Step 2: Run the new tests; expect failures (variants don't exist)**

Run: `cargo test -p arnold-wire 2>&1 | tail -5`

Expected: 4 new tests fail because the `JobSpawned`/`JobCompleted`/`CostUpdate`/`Notification` variants don't exist on `DaemonEvent`.

- [ ] **Step 3: Add the 5 new variants to `DaemonEvent`**

Edit `arnold-wire/src/lib.rs`. The current `DaemonEvent` enum is:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    SessionOpened { session_id: Uuid },
    Reply { session_id: Uuid, text: String },
    TurnComplete { session_id: Uuid },
    Error { session_id: Option<Uuid>, message: String },
    Pong,
}
```

Replace it with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    SessionOpened { session_id: Uuid },
    Reply { session_id: Uuid, text: String },
    TurnComplete { session_id: Uuid },
    Error { session_id: Option<Uuid>, message: String },
    Pong,
    /// A 439 background job has started.
    JobSpawned {
        session_id: Uuid,
        job_id: Uuid,
        prompt_preview: String,
    },
    /// Heartbeat update for an in-flight 439 job.
    JobUpdate {
        session_id: Uuid,
        job_id: Uuid,
        status: String,
        last_log_line: Option<String>,
    },
    /// Terminal event for a 439 job.
    JobCompleted {
        session_id: Uuid,
        job_id: Uuid,
        status: String,
        exit_code: Option<i32>,
        exported_workspace: Option<String>,
        message: String,
    },
    /// Per-turn cost telemetry derived from cpu's `usage` up-frame.
    CostUpdate {
        session_id: Uuid,
        session_cost_usd: f64,
        last_turn_cost_usd: f64,
        last_turn_provider: String,
        last_turn_model: String,
    },
    /// Arnold dispatched an OS notification via `sys_send_notification`.
    Notification {
        session_id: Uuid,
        title: String,
        body: String,
        urgency: String,
    },
}
```

Note: serialize-skip-if-none is intentionally NOT applied to `Option<i32>` / `Option<String>` fields — the TUI's Go decoder expects keys present with `null` rather than absent. Keep default serde behavior.

- [ ] **Step 4: Run tests and verify all pass**

Run: `cargo test -p arnold-wire 2>&1 | tail -5`

Expected: 6 passed (the 2 original `client_request_round_trip` + `daemon_event_round_trip`, plus 4 new variant tests).

- [ ] **Step 5: Commit**

```bash
git add arnold-wire/src/lib.rs
git commit -m "feat(wire): v0b DaemonEvent variants — JobSpawned/Update/Completed, CostUpdate, Notification"
```

---

## Task 2: Job table schema in `jobs.rs`

**Files:**
- Create: `arnoldd/src/jobs.rs`
- Modify: `arnoldd/src/main.rs` (add `mod jobs;`)

The job table records every 439 dispatch with enough state for the TUI to render the jobs pane and for Arnold to look up job status on `sys_poll_job`. It lives in the same SQLite database as the inbox (`~/.arnold/inbox.db`) — both subsystems share connection contention safely under the same `Arc<Mutex<Connection>>` because writes are infrequent.

Schema:

```sql
CREATE TABLE jobs (
    job_id        TEXT PRIMARY KEY,          -- Uuid as string
    session_id    TEXT NOT NULL,             -- Uuid as string; FK-less for now
    prompt        TEXT NOT NULL,             -- original LLM-supplied prompt
    pack_kind     TEXT,                      -- optional, e.g. "nextjs", "rust_cli"
    status        TEXT NOT NULL,             -- queued | running | exporting | completed | failed | cancelled
    exit_code     INTEGER,                   -- exit code from bios; null until terminal
    exported_path TEXT,                      -- ~/.arnold/jobs/<job_id>/workspace once exported
    last_log_line TEXT,                      -- most recent log line from bios stdout for the TUI heartbeat
    message       TEXT,                      -- BIOS' final RunOutcome.message
    started_at    TEXT NOT NULL,             -- rfc3339
    updated_at    TEXT NOT NULL,             -- rfc3339, bumped on every status change
    completed_at  TEXT                       -- rfc3339, set when status moves to a terminal state
);
CREATE INDEX idx_jobs_session ON jobs(session_id, started_at);
CREATE INDEX idx_jobs_active  ON jobs(status) WHERE status IN ('queued','running','exporting');
```

- [ ] **Step 1: Write the failing tests**

Create `arnoldd/src/jobs.rs`:

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    Queued,
    Running,
    Exporting,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Exporting => "exporting",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(JobStatus::Queued),
            "running" => Some(JobStatus::Running),
            "exporting" => Some(JobStatus::Exporting),
            "completed" => Some(JobStatus::Completed),
            "failed" => Some(JobStatus::Failed),
            "cancelled" => Some(JobStatus::Cancelled),
            _ => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled)
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub job_id: Uuid,
    pub session_id: Uuid,
    pub prompt: String,
    pub pack_kind: Option<String>,
    pub status: JobStatus,
    pub exit_code: Option<i32>,
    pub exported_path: Option<String>,
    pub last_log_line: Option<String>,
    pub message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct JobTable(Arc<Mutex<Connection>>);

impl JobTable {
    /// Wrap an existing connection (the same one the Inbox uses) and ensure the
    /// jobs table + indexes exist.
    pub fn attach(conn: Arc<Mutex<Connection>>) -> Result<Self> {
        {
            let c = conn.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
            c.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS jobs (
                    job_id        TEXT PRIMARY KEY,
                    session_id    TEXT NOT NULL,
                    prompt        TEXT NOT NULL,
                    pack_kind     TEXT,
                    status        TEXT NOT NULL,
                    exit_code     INTEGER,
                    exported_path TEXT,
                    last_log_line TEXT,
                    message       TEXT,
                    started_at    TEXT NOT NULL,
                    updated_at    TEXT NOT NULL,
                    completed_at  TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_jobs_session
                    ON jobs(session_id, started_at);
                CREATE INDEX IF NOT EXISTS idx_jobs_active
                    ON jobs(status) WHERE status IN ('queued','running','exporting');
                "#,
            )?;
        }
        Ok(Self(conn))
    }

    pub fn create(&self, session_id: Uuid, prompt: &str, pack_kind: Option<&str>) -> Result<Uuid> {
        let conn = self.0.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let job_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO jobs (job_id, session_id, prompt, pack_kind, status, started_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'queued', ?5, ?5)",
            params![job_id.to_string(), session_id.to_string(), prompt, pack_kind, now],
        )?;
        Ok(job_id)
    }

    pub fn update_status(&self, job_id: Uuid, status: JobStatus, last_log_line: Option<&str>) -> Result<()> {
        let conn = self.0.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();
        if status.is_terminal() {
            conn.execute(
                "UPDATE jobs SET status=?1, last_log_line=?2, updated_at=?3, completed_at=?3
                 WHERE job_id=?4",
                params![status.as_str(), last_log_line, now, job_id.to_string()],
            )?;
        } else {
            conn.execute(
                "UPDATE jobs SET status=?1, last_log_line=?2, updated_at=?3
                 WHERE job_id=?4",
                params![status.as_str(), last_log_line, now, job_id.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn finalize(&self, job_id: Uuid, status: JobStatus, exit_code: Option<i32>, exported_path: Option<&str>, message: &str) -> Result<()> {
        let conn = self.0.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE jobs SET status=?1, exit_code=?2, exported_path=?3, message=?4,
                              updated_at=?5, completed_at=?5
             WHERE job_id=?6",
            params![status.as_str(), exit_code, exported_path, message, now, job_id.to_string()],
        )?;
        Ok(())
    }

    pub fn get(&self, job_id: Uuid) -> Result<Option<Job>> {
        let conn = self.0.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT job_id, session_id, prompt, pack_kind, status, exit_code,
                    exported_path, last_log_line, message, started_at, updated_at, completed_at
             FROM jobs WHERE job_id=?1",
        )?;
        let mut rows = stmt.query(params![job_id.to_string()])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_job(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_active_for_session(&self, session_id: Uuid) -> Result<Vec<Job>> {
        let conn = self.0.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT job_id, session_id, prompt, pack_kind, status, exit_code,
                    exported_path, last_log_line, message, started_at, updated_at, completed_at
             FROM jobs
             WHERE session_id=?1 AND status IN ('queued','running','exporting')
             ORDER BY started_at ASC",
        )?;
        let mut rows = stmt.query(params![session_id.to_string()])?;
        let mut out = vec![];
        while let Some(row) = rows.next()? {
            out.push(row_to_job(row)?);
        }
        Ok(out)
    }
}

fn row_to_job(row: &rusqlite::Row<'_>) -> Result<Job> {
    let job_id: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let status_str: String = row.get(4)?;
    let started_at: String = row.get(9)?;
    let updated_at: String = row.get(10)?;
    let completed_at: Option<String> = row.get(11)?;
    Ok(Job {
        job_id: Uuid::parse_str(&job_id)?,
        session_id: Uuid::parse_str(&session_id)?,
        prompt: row.get(2)?,
        pack_kind: row.get(3)?,
        status: JobStatus::from_str(&status_str)
            .ok_or_else(|| anyhow::anyhow!("unknown job status '{status_str}'"))?,
        exit_code: row.get(5)?,
        exported_path: row.get(6)?,
        last_log_line: row.get(7)?,
        message: row.get(8)?,
        started_at: DateTime::parse_from_rfc3339(&started_at)?.with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated_at)?.with_timezone(&Utc),
        completed_at: completed_at
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_conn(dir: &std::path::Path) -> Arc<Mutex<Connection>> {
        let conn = Connection::open(dir.join("inbox.db")).unwrap();
        Arc::new(Mutex::new(conn))
    }

    #[test]
    fn create_then_get_round_trip() {
        let tmp = TempDir::new().unwrap();
        let table = JobTable::attach(open_conn(tmp.path())).unwrap();
        let sid = Uuid::new_v4();
        let jid = table.create(sid, "make a counter app", Some("rust_cli")).unwrap();
        let job = table.get(jid).unwrap().expect("just-created job missing");
        assert_eq!(job.job_id, jid);
        assert_eq!(job.session_id, sid);
        assert_eq!(job.prompt, "make a counter app");
        assert_eq!(job.pack_kind.as_deref(), Some("rust_cli"));
        assert_eq!(job.status, JobStatus::Queued);
        assert!(job.completed_at.is_none());
    }

    #[test]
    fn finalize_sets_completed_at() {
        let tmp = TempDir::new().unwrap();
        let table = JobTable::attach(open_conn(tmp.path())).unwrap();
        let sid = Uuid::new_v4();
        let jid = table.create(sid, "x", None).unwrap();
        table.finalize(jid, JobStatus::Completed, Some(0), Some("/tmp/exported"), "ok").unwrap();
        let job = table.get(jid).unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert_eq!(job.exit_code, Some(0));
        assert_eq!(job.exported_path.as_deref(), Some("/tmp/exported"));
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn list_active_excludes_terminal() {
        let tmp = TempDir::new().unwrap();
        let table = JobTable::attach(open_conn(tmp.path())).unwrap();
        let sid = Uuid::new_v4();
        let a = table.create(sid, "a", None).unwrap();
        let b = table.create(sid, "b", None).unwrap();
        let c = table.create(sid, "c", None).unwrap();
        table.finalize(b, JobStatus::Completed, Some(0), None, "done").unwrap();
        table.update_status(c, JobStatus::Running, Some("npm install")).unwrap();

        let active = table.list_active_for_session(sid).unwrap();
        assert_eq!(active.len(), 2, "expected 2 active jobs (a queued, c running)");
        let ids: Vec<Uuid> = active.iter().map(|j| j.job_id).collect();
        assert!(ids.contains(&a));
        assert!(ids.contains(&c));
        assert!(!ids.contains(&b), "completed job b should be excluded from active list");
    }
}
```

- [ ] **Step 2: Add `mod jobs;` to `arnoldd/src/main.rs`**

Current module decls in `main.rs`: `config, cpu_process, handler, handlers, inbox, jail, memory, plan_frame, schema, session, syscall, uds_server`. Insert `jobs` between `jail` and `memory` (alphabetical).

- [ ] **Step 3: Run tests**

Run: `cargo test -p arnoldd jobs 2>&1 | tail -10`

Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add arnoldd/src/jobs.rs arnoldd/src/main.rs
git commit -m "feat(arnoldd): jobs table — schema, CRUD, status transitions, active-job listing"
```

---

## Task 3: Usage-frame interceptor (`usage_meter.rs`) + typed `UpFrame::Usage`

**Files:**
- Modify: `arnoldd/src/cpu_process.rs` (promote `usage` from `Other` to a typed variant)
- Create: `arnoldd/src/usage_meter.rs`
- Modify: `arnoldd/src/main.rs` (`mod usage_meter;`)

cpu emits `{"type":"usage", "provider", "model", "role", "task_type", "metrics": {...}, "input_tokens", "output_tokens", "total_tokens", "pricing": {...}, "estimated_cost_usd"?, "actual_cost_usd"?}` after each LLM turn (see `vendor/439/cpu/syscall_loop.cpp:1752-1788`). v0a swallowed this as `UpFrame::Other`. v0b promotes it to its own variant and feeds it into `UsageMeter`, which accumulates per-session cost and emits `DaemonEvent::CostUpdate`.

The provider-error path stays the same string-keyed inspection of the raw Value (it's still rare and unstructured); only `usage` graduates to a typed variant.

- [ ] **Step 1: Update `UpFrame` to add a typed `Usage` variant before the `#[serde(other)]` catch-all**

Edit `arnoldd/src/cpu_process.rs`. Replace:

```rust
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpFrame {
    Syscall { id: u64, method: String, params: Value },
    Finished,
    /// Catch-all for any frame type the v0a daemon doesn't directly handle
    /// (usage, archive_l2, etc.). The session loop pattern-matches the inner
    /// frame to surface provider_error to the user as a DaemonEvent::Error.
    #[serde(other)]
    Other,
}
```

with:

```rust
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpFrame {
    Syscall { id: u64, method: String, params: Value },
    Finished,
    Usage {
        #[serde(default)] provider: String,
        #[serde(default)] model: String,
        #[serde(default)] input_tokens: u64,
        #[serde(default)] output_tokens: u64,
        #[serde(default)] total_tokens: u64,
        #[serde(default)] estimated_cost_usd: Option<f64>,
        #[serde(default)] actual_cost_usd: Option<f64>,
    },
    /// Catch-all for any frame type the daemon doesn't directly handle
    /// (archive_l2, provider_error, etc.). The session loop pattern-matches
    /// the raw Value to surface provider_error as DaemonEvent::Error.
    #[serde(other)]
    Other,
}
```

The `#[serde(default)]` on every field of `Usage` makes the deserializer tolerant of any individual missing field — cpu's usage frame shape evolves frequently and we don't want a missing `pricing` block to drop the entire frame.

- [ ] **Step 2: Write `arnoldd/src/usage_meter.rs`**

```rust
use anyhow::Result;
use arnold_wire::DaemonEvent;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Per-session running cost in USD, accumulated from cpu `usage` up-frames.
/// Resets when the session is closed.
#[derive(Clone)]
pub struct UsageMeter {
    inner: Arc<Mutex<HashMap<Uuid, f64>>>,
}

impl Default for UsageMeter {
    fn default() -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())) }
    }
}

pub struct TurnCost {
    pub session_cost_usd: f64,
    pub last_turn_cost_usd: f64,
    pub last_turn_provider: String,
    pub last_turn_model: String,
}

impl UsageMeter {
    /// Record a turn's cost (preferring `actual_cost_usd` over `estimated_cost_usd`)
    /// and return the new totals for the calling session, or `None` if no cost
    /// number was available (e.g. local Ollama with no override).
    pub fn record(
        &self,
        session_id: Uuid,
        provider: String,
        model: String,
        estimated: Option<f64>,
        actual: Option<f64>,
    ) -> Result<Option<TurnCost>> {
        let last_turn = actual.or(estimated).unwrap_or(0.0);
        if last_turn <= 0.0 && actual.is_none() && estimated.is_none() {
            return Ok(None);
        }
        let mut map = self.inner.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let cumulative = map.entry(session_id).or_insert(0.0);
        *cumulative += last_turn;
        Ok(Some(TurnCost {
            session_cost_usd: *cumulative,
            last_turn_cost_usd: last_turn,
            last_turn_provider: provider,
            last_turn_model: model,
        }))
    }

    /// Drop a session's accumulator. Called on CloseSession.
    pub fn forget(&self, session_id: Uuid) -> Result<()> {
        let mut map = self.inner.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        map.remove(&session_id);
        Ok(())
    }
}

/// Convert a TurnCost into a CostUpdate event the session loop can ship to the client.
pub fn cost_event(session_id: Uuid, tc: &TurnCost) -> DaemonEvent {
    DaemonEvent::CostUpdate {
        session_id,
        session_cost_usd: tc.session_cost_usd,
        last_turn_cost_usd: tc.last_turn_cost_usd,
        last_turn_provider: tc.last_turn_provider.clone(),
        last_turn_model: tc.last_turn_model.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_across_turns() {
        let m = UsageMeter::default();
        let sid = Uuid::new_v4();
        let t1 = m.record(sid, "anthropic".into(), "sonnet-4-6".into(), Some(0.01), None).unwrap().unwrap();
        assert_eq!(t1.last_turn_cost_usd, 0.01);
        assert_eq!(t1.session_cost_usd, 0.01);
        let t2 = m.record(sid, "anthropic".into(), "sonnet-4-6".into(), Some(0.02), None).unwrap().unwrap();
        assert_eq!(t2.last_turn_cost_usd, 0.02);
        assert!((t2.session_cost_usd - 0.03).abs() < 1e-9);
    }

    #[test]
    fn prefers_actual_over_estimated() {
        let m = UsageMeter::default();
        let sid = Uuid::new_v4();
        let t = m.record(sid, "xai".into(), "grok-4.3".into(), Some(0.05), Some(0.0421)).unwrap().unwrap();
        assert!((t.last_turn_cost_usd - 0.0421).abs() < 1e-9);
    }

    #[test]
    fn no_cost_returns_none() {
        let m = UsageMeter::default();
        let sid = Uuid::new_v4();
        let t = m.record(sid, "ollama".into(), "llama3".into(), None, None).unwrap();
        assert!(t.is_none(), "no cost number means no event emitted");
    }

    #[test]
    fn forget_drops_session() {
        let m = UsageMeter::default();
        let sid = Uuid::new_v4();
        m.record(sid, "anthropic".into(), "sonnet-4-6".into(), Some(0.01), None).unwrap();
        m.forget(sid).unwrap();
        // After forget, the next record on a fresh sid starts at zero
        let new_sid = Uuid::new_v4();
        let t = m.record(new_sid, "anthropic".into(), "sonnet-4-6".into(), Some(0.02), None).unwrap().unwrap();
        assert_eq!(t.session_cost_usd, 0.02);
    }
}
```

- [ ] **Step 3: Add `mod usage_meter;` to `main.rs`** alphabetically (between `syscall` and `uds_server`).

- [ ] **Step 4: Run tests**

Run: `cargo test -p arnoldd usage_meter 2>&1 | tail -10`

Expected: 4 passed.

Run: `cargo test -p arnoldd cpu_process 2>&1 | tail -5`

Expected: still 0 tests (cpu_process has no test mod; just verifies the new `Usage` variant compiles).

Run: `cargo build -p arnoldd 2>&1 | tail -3`

Expected: clean compile. Session.rs's existing `Some((UpFrame::Other, raw))` arm will be a non-exhaustive-match compile error if Rust catches it, because we added a new variant. If it does, Task 4 will fix it; for now if it errors, just add a `_ => continue` arm temporarily — but the compiler should accept the existing match because we kept `Other` as the catch-all and `Usage` flows through `Other`'s catch-all check at the wire level (no — actually since `Usage` is a named variant now, the existing match in session.rs needs a new arm or to fall through `Other`). The cleanest path: in Task 4 the session.rs match gets a new `UpFrame::Usage { ... } => ...` arm that calls `usage_meter.record(...)`. If you hit a non-exhaustive-match error in this Step 4 build, just add a placeholder arm `Some((UpFrame::Usage { .. }, _)) => continue,` and let Task 4 replace it.

- [ ] **Step 5: Commit**

```bash
git add arnoldd/src/cpu_process.rs arnoldd/src/usage_meter.rs arnoldd/src/main.rs
git commit -m "feat(arnoldd): typed Usage up-frame variant + UsageMeter for per-session cost"
```

---

## Task 4: Wire `UsageMeter` into the session turn loop

**Files:**
- Modify: `arnoldd/src/session.rs` (consume `UpFrame::Usage`, emit `CostUpdate`)

This task plugs the meter into Arnold's hot path so cost events flow to the client immediately after every cpu turn.

- [ ] **Step 1: Add `UsageMeter` to `DaemonState`**

Edit `arnoldd/src/session.rs`. Update the `DaemonState` struct to carry the meter:

```rust
#[derive(Clone)]
pub struct DaemonState {
    pub config: Arc<ArnoldConfig>,
    pub jail: Arc<Jail>,
    pub memory: Arc<MemoryStore>,
    pub sessions: Arc<Mutex<HashMap<Uuid, SessionState>>>,
    pub usage: crate::usage_meter::UsageMeter,
}
```

And update `DaemonState::new`:

```rust
impl DaemonState {
    pub fn new(config: ArnoldConfig, jail: Jail, memory: MemoryStore) -> Self {
        Self {
            config: Arc::new(config),
            jail: Arc::new(jail),
            memory: Arc::new(memory),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            usage: crate::usage_meter::UsageMeter::default(),
        }
    }
    // ... rest unchanged ...
}
```

Also, in `DaemonState::close`, add a meter-forget call:

```rust
    pub async fn close(&self, id: Uuid) {
        self.sessions.lock().await.remove(&id);
        let _ = self.usage.forget(id);
    }
```

- [ ] **Step 2: Handle `UpFrame::Usage` in the turn loop**

In `handle_user_message`, find the match block that currently looks like:

```rust
    loop {
        let frame_with_raw = cpu.read_up().await?;
        match frame_with_raw {
            None => break,
            Some((UpFrame::Finished, _)) => break,
            Some((UpFrame::Other, raw)) => {
                // ... provider_error handling ...
                continue;
            }
            Some((UpFrame::Syscall { id, method, params }, _)) => {
                // ... dispatch ...
            }
        }
    }
```

Insert a new `UpFrame::Usage` arm BEFORE the `Other` arm:

```rust
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
```

- [ ] **Step 3: Build + run all tests**

Run: `cargo build -p arnoldd 2>&1 | tail -3` — clean.
Run: `cargo test --workspace 2>&1 | tail -5` — all passing (24 from v0a + new tests from Tasks 1-3).

- [ ] **Step 4: Commit**

```bash
git add arnoldd/src/session.rs
git commit -m "feat(session): consume UpFrame::Usage, emit CostUpdate per turn"
```

---

## Task 5: `--check` pre-flight mode + friendlier `rg`-missing diagnostic

**Files:**
- Create: `arnoldd/src/precheck.rs`
- Modify: `arnoldd/src/main.rs` (handle `--check` flag, register `mod precheck;`)
- Modify: `arnoldd/src/handlers/file.rs` (better `rg` spawn error)

`arnoldd --check` spawns the cpu binary, sends a minimal plan frame, then immediately sends `step` followed by a synthetic `result` (or just expects cpu to emit a syscall then echo back finished). The cleanest health check: spawn cpu, send a plan, send a `result` with `{"id":0,"result":{}}` indicating "no syscalls happened, exit cleanly", and read until `finished`. If `cpu` is misconfigured (binary path wrong, provider unreachable, key missing) the error surfaces here instead of on first user turn.

Opus's v0a final-review suggestion #6.

- [ ] **Step 1: Write `arnoldd/src/precheck.rs`**

```rust
use anyhow::{anyhow, Result};
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;
use crate::config::ArnoldConfig;
use crate::cpu_process::{CpuProcess, UpFrame};
use crate::plan_frame::PlanFrameBuilder;

/// Pre-flight a cpu spawn. Spawns the binary, sends a minimal plan frame, then
/// expects cpu to either emit a syscall (which we error-back so cpu exits) or
/// finished. Returns Ok if cpu came up and shut down cleanly.
pub async fn run(config: &ArnoldConfig) -> Result<String> {
    let task_id = Uuid::new_v4();
    let mut cpu = CpuProcess::spawn(
        &config.cpu_binary,
        &config.llm_provider,
        &config.model,
        task_id.to_string(),
    ).await.map_err(|e| anyhow!("failed to spawn cpu: {e}"))?;

    let context = "PROJECT_ROOT: /tmp\nPATH_MODE: project-relative\n\nUSER:\nprecheck\n".to_string();
    let plan = PlanFrameBuilder {
        task_id,
        context,
        model_provider: config.llm_provider.clone(),
        model: config.model.clone(),
    }.build()?;

    cpu.send_plan(plan).await?;
    cpu.send_step().await?;

    // Read frames with a 10s budget; we tell cpu to fail on whatever it emits
    // first so it shuts down quickly. Any syscall we see -> send_error so cpu
    // moves on; finished -> success.
    let budget = Duration::from_secs(10);
    let outcome = timeout(budget, async {
        loop {
            match cpu.read_up().await {
                Ok(None) => return Err(anyhow!("cpu closed stdout before emitting finished")),
                Ok(Some((UpFrame::Finished, _))) => return Ok("cpu finished".to_string()),
                Ok(Some((UpFrame::Syscall { id, .. }, _))) => {
                    let _ = cpu.send_error(id, "precheck mode: not executing syscalls".into()).await;
                    cpu.send_step().await?;
                }
                Ok(Some((UpFrame::Usage { .. }, _))) => continue,
                Ok(Some((UpFrame::Other, _))) => continue,
                Err(e) => return Err(anyhow!("cpu read error: {e}")),
            }
        }
    }).await;

    let result = match outcome {
        Ok(r) => r,
        Err(_) => Err(anyhow!("cpu precheck timed out after {budget:?}")),
    };

    let _ = cpu.shutdown().await;
    result
}
```

- [ ] **Step 2: Handle `--check` in `arnoldd/src/main.rs`**

In `main()`, before the tracing init / config load, peek at `env::args`:

```rust
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("arnoldd=info".parse()?))
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--check") {
        let config = ArnoldConfig::load_or_default()?;
        match precheck::run(&config).await {
            Ok(msg) => {
                println!("arnoldd --check OK: {msg}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("arnoldd --check FAILED: {e:#}");
                std::process::exit(1);
            }
        }
    }

    // ... existing daemon bootstrap unchanged ...
```

Add `mod precheck;` to the module-decl block.

- [ ] **Step 3: Friendlier rg-missing diagnostic in `handlers/file.rs`**

Find the line in `search()`:

```rust
    let mut child = cmd.spawn()?;
```

Replace with:

```rust
    let mut child = cmd.spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => anyhow!(
                "ripgrep (rg) not found on PATH. Install via `brew install ripgrep` (macOS) or your distro's package manager (Linux)."
            ),
            _ => anyhow!("failed to spawn rg: {e}"),
        })?;
```

- [ ] **Step 4: Verify compile**

Run: `cargo build -p arnoldd 2>&1 | tail -3`

Expected: clean.

- [ ] **Step 5: Manual smoke test of `--check`**

The cpu binary should be installed at `~/.arnold/bin/cpu` from v0a's install. Run:

```bash
cargo run -p arnoldd -- --check
```

Expected: prints `arnoldd --check OK: cpu finished` and exits 0. If `ANTHROPIC_API_KEY` is unset or wrong, expect a non-zero exit with a provider-error message.

- [ ] **Step 6: Commit**

```bash
git add arnoldd/src/precheck.rs arnoldd/src/main.rs arnoldd/src/handlers/file.rs
git commit -m "feat(arnoldd): --check pre-flight mode + friendlier rg-missing error"
```

---

## Task 6: BIOS subprocess driver (`bios_process.rs`)

**Files:**
- Create: `arnoldd/src/bios_process.rs`
- Modify: `arnoldd/src/main.rs` (`mod bios_process;`)
- Modify: `arnoldd/Cargo.toml` (`which.workspace = true`)

The BIOS driver spawns `bios run --provider docker --image runtime/os:local --no-build --export-workspace <host_path> -- "<prompt>"` as a subprocess, tails its stdout/stderr lines to update the job table's `last_log_line`, and on exit constructs a `JobCompleted` event from `bios`'s exit code (`bios` returns 0 on success / non-zero on failure; the `RunOutcome` JSON is NOT printed to stdout by the current bios — we parse the exit code only, and the exported workspace path comes from the flag we passed).

**Important contract derivation from `vendor/439/bios/src/runner.rs`:**
- `RunOutcome { kind: OutcomeKind, runtime_exit, exported_workspace: Option<PathBuf>, snapshot, message }`
- `OutcomeKind::Success` ↔ exit 0; `OutcomeKind::RuntimeFailure` ↔ exit non-0
- The exported workspace is the `--export-workspace` path we passed when `bios` finished its export step
- No structured stdout — bios emits human-readable log lines

So Arnold infers:
- If `bios` exits 0 → JobStatus::Completed
- If `bios` exits non-0 → JobStatus::Failed
- `exported_workspace` is the path we passed in (if the export succeeded; if it didn't, bios's stderr would say so)

For v0b we accept the limitation that "successful exit but failed export" is a rare edge case bios already handles internally (it returns an error before exiting 0 in that case).

- [ ] **Step 1: Add `which` to `arnoldd/Cargo.toml`**

In `[dependencies]`, add:

```toml
which = "6"
```

(Also add `which = "6"` to the workspace `[workspace.dependencies]` in the root `Cargo.toml` first so the version stays consistent if other crates pick it up.)

- [ ] **Step 2: Write `arnoldd/src/bios_process.rs`**

```rust
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;
use crate::jobs::{JobStatus, JobTable};

/// Spawn `bios run ...` and drive it to completion. Updates the job table as
/// status transitions happen; returns the terminal (status, exit_code,
/// exported_path, message) tuple for the caller to ship as JobCompleted.
///
/// Arguments:
/// - `bios_binary`: absolute path to the bios executable
/// - `runtime_image`: container image reference (e.g. "runtime/os:local")
/// - `arnold_dir`: Arnold's home (~/.arnold/) — used to derive the export path
/// - `job_id`: UUID of the job this spawn corresponds to
/// - `prompt`: the user-supplied task prompt
/// - `pack_kind`: optional 439 pack hint; currently ignored (bios autodetects)
pub async fn spawn_and_drive(
    bios_binary: &Path,
    runtime_image: &str,
    arnold_dir: &Path,
    table: &JobTable,
    job_id: Uuid,
    prompt: &str,
    pack_kind: Option<&str>,
) -> Result<JobOutcome> {
    let _ = pack_kind; // reserved for future use

    let job_dir = arnold_dir.join("jobs").join(job_id.to_string());
    let export_path = job_dir.join("workspace");
    std::fs::create_dir_all(&job_dir)?;

    // Move from queued to running BEFORE we spawn so the TUI shows the
    // transition immediately even if bios takes a few seconds to start.
    table.update_status(job_id, JobStatus::Running, Some("spawning bios"))?;

    let mut child = Command::new(bios_binary)
        .args([
            "run",
            "--provider", "docker",
            "--image", runtime_image,
            "--no-build",
            "--export-workspace", export_path.to_str()
                .ok_or_else(|| anyhow!("export path is not valid UTF-8: {}", export_path.display()))?,
            "--", prompt,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow!("failed to spawn bios at {}: {e}", bios_binary.display()))?;

    // Tail stdout for heartbeat updates to the job table's last_log_line.
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout on bios"))?;
    let mut stdout_lines = BufReader::new(stdout).lines();
    let table_clone = table.clone();
    let job_id_clone = job_id;
    let log_task = tokio::spawn(async move {
        while let Ok(Some(line)) = stdout_lines.next_line().await {
            // Update last_log_line. Errors are non-fatal — heartbeats are best-effort.
            let _ = table_clone.update_status(
                job_id_clone,
                JobStatus::Running,
                Some(line.chars().take(200).collect::<String>().as_str()),
            );
        }
    });

    // Drain stderr concurrently into a buffer so we can show it on failure.
    let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr on bios"))?;
    let mut stderr_lines = BufReader::new(stderr).lines();
    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        while let Ok(Some(line)) = stderr_lines.next_line().await {
            buf.push_str(&line);
            buf.push('\n');
            if buf.len() > 16_000 {
                // Truncate from the head so we keep the most recent lines.
                let drain_until = buf.len() - 12_000;
                buf.drain(..drain_until);
            }
        }
        buf
    });

    let status = child.wait().await.map_err(|e| anyhow!("waiting on bios: {e}"))?;
    let _ = log_task.await;
    let stderr_tail = stderr_task.await.unwrap_or_default();

    let exit_code = status.code();
    let succeeded = status.success();
    let final_status = if succeeded { JobStatus::Completed } else { JobStatus::Failed };

    // Transition to "exporting" briefly so the TUI shows the export step,
    // then to the terminal status. (Real bios export happens during its own
    // run; this is purely a UX heartbeat for v0b.)
    if succeeded {
        let _ = table.update_status(job_id, JobStatus::Exporting, Some("bios exported workspace"));
    }

    let exported_path_str = if succeeded {
        export_path.to_str().map(String::from)
    } else {
        None
    };

    let message = if succeeded {
        "bios completed; workspace exported".to_string()
    } else {
        format!("bios failed (exit {:?})\nstderr tail:\n{}", exit_code, stderr_tail.trim_end())
    };

    table.finalize(job_id, final_status.clone(), exit_code, exported_path_str.as_deref(), &message)?;

    Ok(JobOutcome {
        job_id,
        status: final_status,
        exit_code,
        exported_path: exported_path_str,
        message,
    })
}

pub struct JobOutcome {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub exit_code: Option<i32>,
    pub exported_path: Option<String>,
    pub message: String,
}

/// Locate the bios binary. Looks at config.bios_binary first, then
/// ~/.arnold/bin/bios, then `which bios`.
pub fn resolve_bios_binary(config_bios_path: Option<&Path>, arnold_dir: &Path) -> Result<PathBuf> {
    if let Some(p) = config_bios_path {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }
    let candidate = arnold_dir.join("bin").join("bios");
    if candidate.exists() {
        return Ok(candidate);
    }
    which::which("bios").map_err(|e| anyhow!("bios binary not found: {e}"))
}
```

- [ ] **Step 3: Add `mod bios_process;` to `arnoldd/src/main.rs`** alphabetically (between `arnold_wire` (which is a use, not a mod, ignore) and `config` — so insert as the first `mod` line: `mod bios_process; mod config; mod cpu_process; ...`).

Wait — actually `bios_process` alphabetically comes between `arnold_wire` (use) and `config`, but `arnold_wire` isn't a `mod`. So as the first `mod` declaration:

```rust
mod bios_process;
mod config;
mod cpu_process;
mod handler;
mod handlers;
mod inbox;
mod jail;
mod jobs;
mod memory;
mod plan_frame;
mod precheck;
mod schema;
mod session;
mod syscall;
mod uds_server;
mod usage_meter;
```

(Note: `precheck` from Task 5 should be in there too, and `usage_meter` from Task 3.)

- [ ] **Step 4: Verify compile**

Run: `cargo build -p arnoldd 2>&1 | tail -3`

Expected: clean compile. `bios_process.rs` has no tests (BIOS spawning requires a real docker daemon; tested end-to-end at v0b smoke test).

- [ ] **Step 5: Commit**

```bash
git add arnoldd/Cargo.toml arnoldd/src/bios_process.rs arnoldd/src/main.rs Cargo.toml
git commit -m "feat(arnoldd): bios subprocess driver — spawn, heartbeat, finalize via JobTable"
```

---

## Task 7: Extend `Syscall` enum with 3 new variants

**Files:**
- Modify: `arnoldd/src/syscall.rs`

This task adds `SpinUp439`, `PollJob`, and `SendNotification` to the typed `Syscall` enum. The schema generator (`schema.rs`) picks them up automatically because schemars derives JsonSchema from the enum.

- [ ] **Step 1: Write the failing tests**

In `arnoldd/src/syscall.rs`, find the existing `#[cfg(test)] mod tests` block and update the `all_methods_matches_enum` test to expect 15 methods (12 v0a + 3 new):

```rust
    #[test]
    fn all_methods_matches_enum() {
        // Sanity: hand-maintained list matches the actual variants
        assert_eq!(Syscall::all_methods().len(), 15);
    }

    #[test]
    fn round_trip_spin_up_439() {
        let s = Syscall::SpinUp439 {
            prompt: "Build a counter app".into(),
            pack_kind: Some("rust_cli".into()),
            export_workspace_to: None,
            env: None,
        };
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["method"], "sys_spin_up_439");
        let back: Syscall = serde_json::from_value(j).unwrap();
        assert!(matches!(back, Syscall::SpinUp439 { .. }));
    }

    #[test]
    fn round_trip_poll_job() {
        let s = Syscall::PollJob { job_id: "abc-123".into() };
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["method"], "sys_poll_job");
        let back: Syscall = serde_json::from_value(j).unwrap();
        assert!(matches!(back, Syscall::PollJob { .. }));
    }

    #[test]
    fn round_trip_send_notification() {
        let s = Syscall::SendNotification {
            title: "Test".into(),
            body: "Body".into(),
            urgency: None,
        };
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["method"], "sys_send_notification");
    }
```

- [ ] **Step 2: Run tests; expect failures (variants don't exist)**

Run: `cargo test -p arnoldd syscall 2>&1 | tail -10`

Expected: 4 tests, 3 fail (new variants not present), 1 fails (length assertion expects 15, gets 12).

- [ ] **Step 3: Add the 3 new variants to `Syscall`**

Edit `arnoldd/src/syscall.rs`. Locate the `Syscall` enum and insert these variants BEFORE the closing brace (after `MemoryList {}`):

```rust
    // v0b: 439 dispatch
    #[serde(rename = "sys_spin_up_439")]
    SpinUp439 {
        prompt: String,
        #[serde(default)] pack_kind: Option<String>,
        #[serde(default)] export_workspace_to: Option<String>,
        #[serde(default)] env: Option<std::collections::HashMap<String, String>>,
    },
    #[serde(rename = "sys_poll_job")]
    PollJob { job_id: String },

    // v0b: notifications
    #[serde(rename = "sys_send_notification")]
    SendNotification {
        title: String,
        body: String,
        #[serde(default)] urgency: Option<String>,
    },
```

Update `Syscall::method()` to map the new variants:

```rust
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
            SpinUp439 { .. } => "sys_spin_up_439",
            PollJob { .. } => "sys_poll_job",
            SendNotification { .. } => "sys_send_notification",
        }
    }

    pub fn all_methods() -> &'static [&'static str] {
        &[
            "sys_reply", "sys_done",
            "sys_read_file", "sys_write_file", "sys_replace_in_file", "sys_list_dir", "sys_search",
            "sys_run_command",
            "sys_web_fetch",
            "sys_memory_read", "sys_memory_write", "sys_memory_list",
            "sys_spin_up_439", "sys_poll_job",
            "sys_send_notification",
        ]
    }
}
```

- [ ] **Step 4: Run tests; expect 5 passed**

Run: `cargo test -p arnoldd syscall 2>&1 | tail -10`

Expected: 5 passed (the 2 original + 3 new).

Also run the schema tests since the JSON Schema picked up new branches:

Run: `cargo test -p arnoldd schema 2>&1 | tail -5`

Expected: 5 passed (still); the existing `generates_schema_with_only_allowed_methods` test filters to a specific subset so doesn't fail on count.

- [ ] **Step 5: The handler dispatcher won't compile yet — it's missing arms for the 3 new variants**

Run: `cargo check -p arnoldd 2>&1 | tail -10`

Expected: an error like "non-exhaustive patterns: `SpinUp439 { .. }`, `PollJob { .. }`, `SendNotification { .. }` not covered" in `handler.rs::dispatch`. That's expected — Task 8 adds the handler module + dispatch arms.

For this task's commit, add temporary placeholder arms to `handler.rs::dispatch` so the workspace builds. Edit `arnoldd/src/handler.rs`:

```rust
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
        // v0b — implementations in Task 8 / Task 9
        SpinUp439 { prompt, pack_kind, export_workspace_to, env } =>
            crate::handlers::dispatch_439::spin_up_439(ctx, prompt, pack_kind, export_workspace_to, env).await,
        PollJob { job_id } =>
            crate::handlers::dispatch_439::poll_job(ctx, job_id).await,
        SendNotification { title, body, urgency } =>
            crate::handlers::notifications::send_notification(ctx, title, body, urgency).await,
    }
}
```

Then stub the new handlers (Task 8 fills them in):

```bash
mkdir -p arnoldd/src/handlers
```

Create `arnoldd/src/handlers/dispatch_439.rs`:

```rust
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use crate::handler::HandlerContext;

pub async fn spin_up_439(
    _ctx: &HandlerContext,
    _prompt: String,
    _pack_kind: Option<String>,
    _export_workspace_to: Option<String>,
    _env: Option<HashMap<String, String>>,
) -> Result<Value> { unimplemented!("Task 8") }

pub async fn poll_job(_ctx: &HandlerContext, _job_id: String) -> Result<Value> { unimplemented!("Task 8") }
```

Create `arnoldd/src/handlers/notifications.rs`:

```rust
use anyhow::Result;
use serde_json::Value;
use crate::handler::HandlerContext;

pub async fn send_notification(
    _ctx: &HandlerContext,
    _title: String,
    _body: String,
    _urgency: Option<String>,
) -> Result<Value> { unimplemented!("Task 9") }
```

Update `arnoldd/src/handlers/mod.rs`:

```rust
pub mod lifecycle;
pub mod file;
pub mod shell;
pub mod web;
pub mod memory_syscalls;
pub mod dispatch_439;
pub mod notifications;
```

- [ ] **Step 6: Run `cargo build -p arnoldd 2>&1 | tail -3`**

Expected: clean compile (the unimplemented stubs satisfy the dispatcher's type signatures).

- [ ] **Step 7: Commit**

```bash
git add arnoldd/src/syscall.rs arnoldd/src/handler.rs arnoldd/src/handlers/mod.rs arnoldd/src/handlers/dispatch_439.rs arnoldd/src/handlers/notifications.rs
git commit -m "feat(syscall): v0b variants — sys_spin_up_439, sys_poll_job, sys_send_notification (handlers stubbed)"
```

---

## Task 8: `sys_spin_up_439` + `sys_poll_job` handler implementations

**Files:**
- Modify: `arnoldd/src/handlers/dispatch_439.rs` (replace stubs from Task 7)
- Modify: `arnoldd/src/handler.rs` (`HandlerContext` needs `jobs` + `state` access)
- Modify: `arnoldd/src/session.rs` (extend HandlerContext build to pass through job table + bios path)
- Modify: `arnoldd/src/config.rs` (add `bios_binary` + `runtime_image` config fields)

`sys_spin_up_439` is the headline async syscall: it returns `{ job_id }` immediately, and the BIOS run happens in a `tokio::spawn`'d background task that eventually pushes a `JobCompleted` event back through the session's client channel.

`sys_poll_job` is a sync read of the job table.

- [ ] **Step 1: Add config fields for bios binary + runtime image**

Edit `arnoldd/src/config.rs`. Add to `ArnoldConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArnoldConfig {
    pub cpu_binary: PathBuf,
    pub bios_binary: PathBuf,          // NEW
    pub runtime_image: String,         // NEW
    pub llm_provider: String,
    pub model: String,
    pub allowed_dirs: Vec<PathBuf>,
}

impl Default for ArnoldConfig {
    fn default() -> Self {
        let home = dirs::home_dir().expect("no home dir");
        Self {
            cpu_binary: home.join(".arnold/bin/cpu"),
            bios_binary: home.join(".arnold/bin/bios"),        // NEW
            runtime_image: "runtime/os:local".to_string(),     // NEW
            llm_provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            allowed_dirs: vec![],
        }
    }
}
```

- [ ] **Step 2: Extend `HandlerContext` to carry job table + bios config**

Edit `arnoldd/src/handler.rs`:

```rust
use anyhow::Result;
use arnold_wire::DaemonEvent;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use crate::syscall::Syscall;
use crate::jail::Jail;
use crate::jobs::JobTable;
use crate::memory::MemoryStore;
use crate::session::{ClientSender, SessionState};

pub struct HandlerContext {
    pub jail: Arc<Jail>,
    pub memory: Arc<MemoryStore>,
    pub jobs: JobTable,
    pub bios_binary: PathBuf,
    pub runtime_image: String,
    pub arnold_dir: PathBuf,
    pub session: SessionState,
    /// Cloned sender so async background tasks (e.g. bios driver) can push
    /// JobCompleted events without owning the session.
    pub client: ClientSender,
}

// dispatch() body unchanged
```

- [ ] **Step 3: Update `DaemonState` and the turn-handler ctx construction**

Edit `arnoldd/src/session.rs`. `DaemonState` already has `jobs` per Task 2 — actually no, Task 2 created `JobTable` but didn't add it to `DaemonState`. Add it now:

```rust
#[derive(Clone)]
pub struct DaemonState {
    pub config: Arc<ArnoldConfig>,
    pub jail: Arc<Jail>,
    pub memory: Arc<MemoryStore>,
    pub jobs: JobTable,
    pub arnold_dir: Arc<PathBuf>,
    pub sessions: Arc<Mutex<HashMap<Uuid, SessionState>>>,
    pub usage: crate::usage_meter::UsageMeter,
}

impl DaemonState {
    pub fn new(config: ArnoldConfig, jail: Jail, memory: MemoryStore, jobs: JobTable, arnold_dir: PathBuf) -> Self {
        Self {
            config: Arc::new(config),
            jail: Arc::new(jail),
            memory: Arc::new(memory),
            jobs,
            arnold_dir: Arc::new(arnold_dir),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            usage: crate::usage_meter::UsageMeter::default(),
        }
    }
    // open/get/close unchanged
}
```

In `handle_user_message`, update the HandlerContext construction:

```rust
    let ctx = HandlerContext {
        jail: state.jail.clone(),
        memory: state.memory.clone(),
        jobs: state.jobs.clone(),
        bios_binary: state.config.bios_binary.clone(),
        runtime_image: state.config.runtime_image.clone(),
        arnold_dir: (*state.arnold_dir).clone(),
        session: session.clone(),
        client: session.client.clone(),
    };
```

Update `arnoldd/src/main.rs` to construct `JobTable` from the inbox connection and pass `arnold_dir` through:

```rust
    let config = ArnoldConfig::load_or_default()?;
    let arnold_dir = ArnoldConfig::arnold_dir()?;
    let sock_path = ArnoldConfig::socket_path()?;
    let inbox = Inbox::open(&arnold_dir.join("inbox.db"))?;
    let jobs = jobs::JobTable::attach(inbox.connection())?;  // see Step 4 note
    let memory = MemoryStore::open(&arnold_dir.join("memory"))?;
    let mut jail_roots = vec![arnold_dir.join("workspace")];
    jail_roots.extend(config.allowed_dirs.iter().cloned());
    std::fs::create_dir_all(&jail_roots[0])?;
    for p in &config.allowed_dirs {
        if let Err(e) = p.canonicalize() {
            tracing::warn!(/* unchanged */);
        }
    }
    let jail = Jail::new(jail_roots);

    let state = DaemonState::new(config, jail, memory, jobs, arnold_dir.clone());
```

- [ ] **Step 4: Expose `Inbox::connection()` so `JobTable` can share the same SQLite connection**

Edit `arnoldd/src/inbox.rs`. Add a method to `Inbox`:

```rust
impl Inbox {
    // ... existing methods ...

    /// Return a clone of the shared connection handle so other subsystems
    /// (e.g. JobTable) can attach to the same SQLite database without opening
    /// a second file handle.
    pub fn connection(&self) -> std::sync::Arc<std::sync::Mutex<rusqlite::Connection>> {
        self.0.clone()
    }
}
```

- [ ] **Step 5: Implement `spin_up_439` and `poll_job` in `handlers/dispatch_439.rs`**

Replace `arnoldd/src/handlers/dispatch_439.rs`:

```rust
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
    let _ = ctx.client.send(DaemonEvent::JobSpawned {
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
    let client = ctx.client.clone();
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

    Ok(json!({ "job_id": job_id.to_string(), "status": "running" }))
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
```

- [ ] **Step 6: Build + test**

Run: `cargo build -p arnoldd 2>&1 | tail -3` — clean.
Run: `cargo test --workspace 2>&1 | tail -5` — all tests still pass (no new tests in this task; spin_up_439 is exercised in the v0b smoke test at the end).

- [ ] **Step 7: Commit**

```bash
git add arnoldd/src/handlers/dispatch_439.rs arnoldd/src/handler.rs arnoldd/src/session.rs arnoldd/src/main.rs arnoldd/src/config.rs arnoldd/src/inbox.rs
git commit -m "feat(handlers): sys_spin_up_439 spawns bios in background; sys_poll_job reads job table"
```

---

## Task 9: `sys_send_notification` handler + platform-specific dispatch

**Files:**
- Modify: `arnoldd/src/handlers/notifications.rs` (replace Task 7 stub)
- Create: `arnoldd/src/notify.rs` (platform dispatcher)
- Modify: `arnoldd/src/main.rs` (`mod notify;`)
- Modify: `arnoldd/Cargo.toml` (`notify-rust.workspace = true`)

OS notification dispatch:
- macOS: `osascript -e 'display notification "<body>" with title "<title>"'` (cleanest, no deps)
- Linux: `notify-send <title> <body>` (libnotify CLI; available in any modern desktop env)

For both platforms, `notify-rust` is also acceptable as a Rust-native wrapper, but it pulls in DBus dependencies on Linux that we don't need for a sub-process shell-out. v0b uses the subprocess approach — simpler, less linkage, no DBus.

The handler also emits a `DaemonEvent::Notification` event so the TUI sees it (separate from the OS notification — the TUI may want to render its own indicator).

- [ ] **Step 1: Add `notify-rust` to the workspace Cargo.toml** (reserved for v0.1 even though v0b uses subprocess; declaring it now means no Cargo.toml churn later when we want to switch)

In root `Cargo.toml` `[workspace.dependencies]`:

```toml
notify-rust = "4"
```

In `arnoldd/Cargo.toml` `[dependencies]`: NO ENTRY — we don't actually depend on it for v0b. Leave it workspace-declared but un-imported. (If you want to skip the workspace decl entirely and add it directly later, that's fine.)

- [ ] **Step 2: Write `arnoldd/src/notify.rs`**

```rust
use anyhow::{anyhow, Result};
use std::process::Stdio;
use tokio::process::Command;

/// Dispatch an OS notification via the platform-native CLI.
///
/// On macOS, shells out to `osascript`. On Linux, shells out to `notify-send`.
/// On other platforms, returns an error.
///
/// The `urgency` parameter is a freeform string with defined values:
/// "low" | "normal" | "critical" (matching freedesktop standards). macOS
/// ignores urgency; Linux passes it through to notify-send.
pub async fn dispatch(title: &str, body: &str, urgency: Option<&str>) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification {body} with title {title}",
            body = escape_applescript(body),
            title = escape_applescript(title),
        );
        let status = Command::new("osascript")
            .arg("-e").arg(&script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .status()
            .await
            .map_err(|e| anyhow!("osascript not found: {e}"))?;
        if !status.success() {
            return Err(anyhow!("osascript failed with exit {:?}", status.code()));
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("notify-send");
        if let Some(u) = urgency {
            cmd.arg("--urgency").arg(u);
        }
        cmd.arg(title).arg(body)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let status = cmd.status().await
            .map_err(|e| anyhow!("notify-send not found: {e}"))?;
        if !status.success() {
            return Err(anyhow!("notify-send failed with exit {:?}", status.code()));
        }
        return Ok(());
    }

    #[allow(unreachable_code)]
    {
        let _ = urgency;
        Err(anyhow!("OS notifications are only supported on macOS and Linux in v0b"))
    }
}

#[cfg(target_os = "macos")]
fn escape_applescript(s: &str) -> String {
    // Double-quote and escape backslashes/quotes for AppleScript string literals.
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
```

- [ ] **Step 3: Implement `send_notification` handler**

Replace `arnoldd/src/handlers/notifications.rs`:

```rust
use anyhow::Result;
use arnold_wire::DaemonEvent;
use serde_json::{json, Value};
use crate::handler::HandlerContext;
use crate::notify;

pub async fn send_notification(
    ctx: &HandlerContext,
    title: String,
    body: String,
    urgency: Option<String>,
) -> Result<Value> {
    let urgency_str = urgency.clone().unwrap_or_else(|| "normal".to_string());

    // Best-effort OS dispatch — if the platform CLI is missing we still emit
    // the in-app event so the TUI shows it.
    let os_result = notify::dispatch(&title, &body, urgency.as_deref()).await;

    // Always emit the wire event regardless of OS dispatch success.
    let _ = ctx.client.send(DaemonEvent::Notification {
        session_id: ctx.session.session_id,
        title: title.clone(),
        body: body.clone(),
        urgency: urgency_str.clone(),
    });

    match os_result {
        Ok(()) => Ok(json!({ "status": "dispatched" })),
        Err(e) => Ok(json!({
            "status": "tui_only",
            "note": format!("OS notification failed but TUI event emitted: {e}"),
        })),
    }
}
```

- [ ] **Step 4: Add `mod notify;` to `arnoldd/src/main.rs`** alphabetically (between `memory` and `plan_frame`).

- [ ] **Step 5: Build + test**

Run: `cargo build -p arnoldd 2>&1 | tail -3` — clean.

No unit test for `notify::dispatch` — it shells out to platform binaries that may not be available in CI. Tested manually at v0b smoke test.

- [ ] **Step 6: Commit**

```bash
git add arnoldd/src/notify.rs arnoldd/src/handlers/notifications.rs arnoldd/src/main.rs Cargo.toml
git commit -m "feat(handlers): sys_send_notification via osascript (macOS) / notify-send (Linux) + wire event"
```

---

## Task 10: Update `ROLE.md` + `plan_frame.rs` syscall menu for v0b

**Files:**
- Modify: `role/ROLE.md`
- Modify: `arnoldd/src/plan_frame.rs`

The LLM needs to know about the 3 new syscalls. ROLE.md gets updated sections; the plan-frame's `arnold_syscall_menu()` gets 3 new SyscallSpec entries with descriptions + JSON examples.

- [ ] **Step 1: Update `role/ROLE.md`**

In the `allowed_syscalls:` YAML list, add three entries at the bottom:

```yaml
  - sys_spin_up_439
  - sys_poll_job
  - sys_send_notification
```

Remove the "What's not in this version (v0a)" section's first three bullets (the 439 bullet, the schedule/cancel/notification bullet — keep the calendar/mail/browser bullet) and replace with this new v0b section after the "What you do" block:

```markdown
## 439 dispatch

You can spin up an ephemeral 439 environment for a build task with `sys_spin_up_439`. The call returns a `job_id` immediately and the build runs in the background; completion arrives as an inbox event on a future turn. While a job runs, you can keep talking with the user normally, poll status with `sys_poll_job`, or start additional jobs (up to the daemon's concurrency cap of 3).

When the user asks you to build, refactor, or test a project that fits a 439 pack:
- Use `sys_spin_up_439 { prompt: "<the user's goal as a clear task>", pack_kind: "<optional 439 pack hint>" }`.
- Tell the user the job is started, then `sys_done` and wait.
- On the next turn, the inbox will tell you whether it completed. Report the exported workspace path back to the user.
- If the user wants iteration, spin up another job with the refined prompt.

## Notifications

For long-running work, use `sys_send_notification { title, body }` to nudge the user. Examples: a 439 job has just completed and the user moved away, a slow `sys_run_command` is finishing, an unexpected error needs attention. Don't spam — once per coherent event.
```

Also update the v0a "What's not in this version" list to remove the now-implemented items. The new list should be:

```markdown
## What's not in this version (v0b)

- You cannot schedule future work or cancel in-flight 439 jobs. Those land in v0.1.
- You have no calendar, mail, or browser tools. Future versions.
```

- [ ] **Step 2: Add the 3 new SyscallSpec entries to `arnold_syscall_menu()` in `arnoldd/src/plan_frame.rs`**

Find the `vec![ ... ]` block in `arnold_syscall_menu()` and append three more entries before the closing `]`:

```rust
        mk("sys_spin_up_439", "Spin up a 439 environment to handle a build/refactor/test task in the background. Returns a job_id immediately; completion is delivered as a future inbox event. While the job runs, you can keep replying to the user.",
            json!({"method":"sys_spin_up_439","params":{"prompt":"Build a Rust CLI counter app","pack_kind":"rust_cli"}})),
        mk("sys_poll_job", "Get the current status, last log line, and (if complete) the exported workspace path for a previously-started 439 job.",
            json!({"method":"sys_poll_job","params":{"job_id":"abc-123-def-456"}})),
        mk("sys_send_notification", "Send an OS-level notification (macOS Notification Center / Linux libnotify) to nudge the user. Title and body are plain strings.",
            json!({"method":"sys_send_notification","params":{"title":"Job complete","body":"Counter app build succeeded","urgency":"normal"}})),
```

- [ ] **Step 3: Update the plan-frame test assertion**

Find:

```rust
        assert_eq!(frame["syscalls"].as_array().unwrap().len(), 12);
```

Replace with:

```rust
        assert_eq!(frame["syscalls"].as_array().unwrap().len(), 15);
```

- [ ] **Step 4: Build + test**

Run: `cargo test -p arnoldd plan_frame 2>&1 | tail -5`

Expected: 1 passed.

Run: `cargo build -p arnoldd 2>&1 | tail -3`

Expected: clean. `role/ROLE.md` is `include_str!`'d, so changes are picked up automatically.

- [ ] **Step 5: Commit**

```bash
git add role/ROLE.md arnoldd/src/plan_frame.rs
git commit -m "feat(role): update ROLE.md + syscall menu for v0b — sys_spin_up_439, sys_poll_job, sys_send_notification"
```

---

## Task 11: Go TUI module scaffolding (`arnold-tui/`)

**Files:**
- Create: `arnold-tui/go.mod`
- Create: `arnold-tui/main.go` (minimal main)
- Create: `arnold-tui/wire/wire.go` (mirror of arnold-wire types)

The Go module is a sibling of the Cargo workspace. Go and Rust don't share build tooling; running `go build ./arnold-tui/...` is independent of `cargo build`.

- [ ] **Step 1: Initialize the Go module**

```bash
cd /Users/joshua/Codebases/Arnold/arnold-tui
go mod init github.com/jbankse/arnold/arnold-tui
go get github.com/charmbracelet/bubbletea@latest
go get github.com/charmbracelet/bubbles@latest
go get github.com/charmbracelet/lipgloss@latest
go get github.com/google/uuid@latest
cd ..
```

Expected: `arnold-tui/go.mod` and `arnold-tui/go.sum` created with the four deps pinned.

- [ ] **Step 2: Write `arnold-tui/wire/wire.go`** — type mirrors of arnold-wire

```go
package wire

import (
	"encoding/json"

	"github.com/google/uuid"
)

// ClientRequest is the tagged union sent from CLI to daemon.
// Mirrors arnold-wire/src/lib.rs's ClientRequest enum with snake_case tags.
type ClientRequest struct {
	Type      string     `json:"type"` // one of: open_session, user_message, close_session, ping
	Cwd       string     `json:"cwd,omitempty"`
	SessionID *uuid.UUID `json:"session_id,omitempty"`
	Text      string     `json:"text,omitempty"`
}

// Constructors avoid stringly-typed Type fields at call sites.

func OpenSession(cwd string) ClientRequest {
	return ClientRequest{Type: "open_session", Cwd: cwd}
}
func UserMessage(sessionID uuid.UUID, text string) ClientRequest {
	return ClientRequest{Type: "user_message", SessionID: &sessionID, Text: text}
}
func CloseSession(sessionID uuid.UUID) ClientRequest {
	return ClientRequest{Type: "close_session", SessionID: &sessionID}
}
func Ping() ClientRequest {
	return ClientRequest{Type: "ping"}
}

// DaemonEvent is the union from daemon to CLI. Type-tagged like ClientRequest.
type DaemonEvent struct {
	Type      string     `json:"type"`
	SessionID *uuid.UUID `json:"session_id,omitempty"`

	// Reply / TurnComplete / Error
	Text    string `json:"text,omitempty"`
	Message string `json:"message,omitempty"`

	// JobSpawned / JobUpdate / JobCompleted
	JobID             *uuid.UUID `json:"job_id,omitempty"`
	PromptPreview     string     `json:"prompt_preview,omitempty"`
	Status            string     `json:"status,omitempty"`
	LastLogLine       *string    `json:"last_log_line,omitempty"`
	ExitCode          *int       `json:"exit_code,omitempty"`
	ExportedWorkspace *string    `json:"exported_workspace,omitempty"`

	// CostUpdate
	SessionCostUSD   float64 `json:"session_cost_usd,omitempty"`
	LastTurnCostUSD  float64 `json:"last_turn_cost_usd,omitempty"`
	LastTurnProvider string  `json:"last_turn_provider,omitempty"`
	LastTurnModel    string  `json:"last_turn_model,omitempty"`

	// Notification
	Title   string `json:"title,omitempty"`
	Body    string `json:"body,omitempty"`
	Urgency string `json:"urgency,omitempty"`
}

// DecodeEvent reads one JSON-line frame.
func DecodeEvent(b []byte) (DaemonEvent, error) {
	var ev DaemonEvent
	err := json.Unmarshal(b, &ev)
	return ev, err
}

// Encode writes one JSON-line frame (caller appends \n).
func (r ClientRequest) Encode() ([]byte, error) {
	return json.Marshal(r)
}
```

Note: this is a "fat struct" that holds every possible field across all variants. Go doesn't have native sum types, and the dispatcher in `tui/model.go` switches on `Type`. This is the conventional Go pattern; type safety lives at the switch site rather than at the type definition. The cost is a few extra empty fields per decoded event, which is negligible.

- [ ] **Step 3: Write a minimal `arnold-tui/main.go` that just verifies the wire decode works**

```go
package main

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/jbankse/arnold/arnold-tui/wire"
)

func main() {
	// Smoke test: decode a known good frame.
	sample := `{"type":"reply","session_id":"00000000-0000-0000-0000-000000000000","text":"hi"}`
	ev, err := wire.DecodeEvent([]byte(sample))
	if err != nil {
		fmt.Fprintf(os.Stderr, "decode failed: %v\n", err)
		os.Exit(1)
	}
	out, _ := json.MarshalIndent(ev, "", "  ")
	fmt.Println(string(out))
}
```

- [ ] **Step 4: Verify build + run**

```bash
cd /Users/joshua/Codebases/Arnold/arnold-tui
go build ./...
./arnold-tui
```

Expected: prints a pretty-printed JSON of the decoded event including `"type": "reply"`, `"text": "hi"`. Confirms the wire types parse the same JSON shape arnoldd emits.

- [ ] **Step 5: Commit**

```bash
cd /Users/joshua/Codebases/Arnold
git add arnold-tui/go.mod arnold-tui/go.sum arnold-tui/main.go arnold-tui/wire/wire.go
git commit -m "feat(arnold-tui): Go module + wire type mirrors of arnold-wire"
```

---

## Task 12: UDS client (`arnold-tui/client/client.go`)

**Files:**
- Create: `arnold-tui/client/client.go`

The Go UDS client connects to `~/.arnold/arnold.sock`, sends `ClientRequest` frames, and streams `DaemonEvent` frames on a channel.

- [ ] **Step 1: Write `arnold-tui/client/client.go`**

```go
package client

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"sync"

	"github.com/jbankse/arnold/arnold-tui/wire"
)

// Client is a connection to arnoldd over UDS. Send writes one request frame
// per call (newline-delimited JSON); Events returns a read-only channel that
// produces decoded DaemonEvents until the connection closes.
type Client struct {
	conn   net.Conn
	w      *bufio.Writer
	events <-chan wire.DaemonEvent
	errs   <-chan error
	closed chan struct{}
	once   sync.Once
}

func socketPath() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, ".arnold", "arnold.sock"), nil
}

// Dial connects to the daemon. Returns an error if the socket doesn't exist
// (typically: daemon not running) — caller can show "is arnoldd running?" UX.
func Dial(ctx context.Context) (*Client, error) {
	path, err := socketPath()
	if err != nil {
		return nil, err
	}
	d := net.Dialer{}
	conn, err := d.DialContext(ctx, "unix", path)
	if err != nil {
		return nil, fmt.Errorf("cannot connect to %s (%w); is arnoldd running?", path, err)
	}

	events := make(chan wire.DaemonEvent, 32)
	errs := make(chan error, 1)
	closed := make(chan struct{})

	c := &Client{
		conn:   conn,
		w:      bufio.NewWriter(conn),
		events: events,
		errs:   errs,
		closed: closed,
	}

	go c.readLoop(events, errs, closed)
	return c, nil
}

// Send writes one request frame followed by '\n'.
func (c *Client) Send(req wire.ClientRequest) error {
	b, err := req.Encode()
	if err != nil {
		return err
	}
	if _, err := c.w.Write(b); err != nil {
		return err
	}
	if err := c.w.WriteByte('\n'); err != nil {
		return err
	}
	return c.w.Flush()
}

// Events returns the channel events arrive on. Closes when the daemon hangs up.
func (c *Client) Events() <-chan wire.DaemonEvent { return c.events }

// Errs returns a one-shot channel that fires if the read loop crashed.
func (c *Client) Errs() <-chan error { return c.errs }

// Close half-closes the write side and waits for read to drain.
func (c *Client) Close() error {
	c.once.Do(func() {
		_ = c.conn.Close()
		<-c.closed
	})
	return nil
}

func (c *Client) readLoop(events chan<- wire.DaemonEvent, errs chan<- error, closed chan<- struct{}) {
	defer close(events)
	defer close(closed)
	dec := bufio.NewScanner(c.conn)
	dec.Buffer(make([]byte, 0, 1024*1024), 16*1024*1024) // up to 16MB per line for large memory bodies
	for dec.Scan() {
		line := dec.Bytes()
		if len(line) == 0 {
			continue
		}
		var ev wire.DaemonEvent
		if err := json.Unmarshal(line, &ev); err != nil {
			// Drop malformed lines; signal but don't crash.
			select {
			case errs <- fmt.Errorf("malformed daemon frame: %w", err):
			default:
			}
			continue
		}
		events <- ev
	}
	if err := dec.Err(); err != nil && !errors.Is(err, net.ErrClosed) {
		select {
		case errs <- err:
		default:
		}
	}
}
```

- [ ] **Step 2: Verify build**

```bash
cd /Users/joshua/Codebases/Arnold/arnold-tui
go build ./...
```

Expected: clean compile.

- [ ] **Step 3: Commit**

```bash
cd /Users/joshua/Codebases/Arnold
git add arnold-tui/client/client.go
git commit -m "feat(arnold-tui): UDS client with framed read loop and request-send helpers"
```

---

## Task 13: Root bubbletea model (`arnold-tui/tui/model.go`)

**Files:**
- Create: `arnold-tui/tui/model.go`
- Create: `arnold-tui/tui/keybindings.go`

The root model owns:
- Connection state (connected / disconnected / connecting)
- Session ID (once SessionOpened arrives)
- Pane focus (Conversation / Inbox / Jobs)
- Sub-models for each pane (created in Tasks 14-16; for this task we stub them as empty placeholders)
- A persistent message log (in-memory ring of conversation events)
- An input buffer for the conversation pane

Per-tick: the model receives `tea.Msg` updates either from keyboard input or from a goroutine that forwards `client.Events()` into the bubbletea message channel.

- [ ] **Step 1: Write `arnold-tui/tui/keybindings.go`**

```go
package tui

import (
	tea "github.com/charmbracelet/bubbletea"
)

// PaneID identifies which pane has focus.
type PaneID int

const (
	PaneConversation PaneID = iota
	PaneInbox
	PaneJobs
)

func (p PaneID) String() string {
	switch p {
	case PaneConversation:
		return "conversation"
	case PaneInbox:
		return "inbox"
	case PaneJobs:
		return "jobs"
	}
	return "?"
}

// Mode is vim-ish: Normal navigates, Insert types into the input buffer.
type Mode int

const (
	ModeNormal Mode = iota
	ModeInsert
)

// nextPane / prevPane wrap around the 3 panes.
func nextPane(p PaneID) PaneID {
	switch p {
	case PaneConversation:
		return PaneInbox
	case PaneInbox:
		return PaneJobs
	case PaneJobs:
		return PaneConversation
	}
	return PaneConversation
}

func prevPane(p PaneID) PaneID {
	switch p {
	case PaneConversation:
		return PaneJobs
	case PaneInbox:
		return PaneConversation
	case PaneJobs:
		return PaneInbox
	}
	return PaneConversation
}

// rootKeybindings handles global keys (mode changes, pane focus, quit, help).
// Pane-specific keys are handled in each pane's Update.
func (m *Model) rootKeybindings(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	if m.mode == ModeInsert {
		switch msg.Type {
		case tea.KeyEsc:
			m.mode = ModeNormal
			return m, nil
		case tea.KeyEnter:
			text := m.input
			m.input = ""
			m.mode = ModeNormal
			return m, m.sendUserMessage(text)
		case tea.KeyBackspace:
			if len(m.input) > 0 {
				m.input = m.input[:len(m.input)-1]
			}
			return m, nil
		default:
			if msg.Type == tea.KeyRunes {
				m.input += string(msg.Runes)
			}
			return m, nil
		}
	}
	// Normal mode
	switch msg.String() {
	case "q", "ctrl+c":
		return m, tea.Quit
	case "i":
		if m.focus == PaneConversation {
			m.mode = ModeInsert
		}
		return m, nil
	case "tab":
		m.focus = nextPane(m.focus)
		return m, nil
	case "shift+tab":
		m.focus = prevPane(m.focus)
		return m, nil
	case "?":
		m.helpOpen = !m.helpOpen
		return m, nil
	}
	return m, nil
}
```

- [ ] **Step 2: Write `arnold-tui/tui/model.go`**

```go
package tui

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/google/uuid"

	"github.com/jbankse/arnold/arnold-tui/client"
	"github.com/jbankse/arnold/arnold-tui/wire"
)

// Conn state for the top-of-screen status badge.
type ConnState int

const (
	ConnConnecting ConnState = iota
	ConnConnected
	ConnDisconnected
)

// MessageRow is one rendered line in the conversation pane.
type MessageRow struct {
	Author string // "you" | "arnold" | "system"
	Text   string
}

// Job is a row in the jobs pane.
type Job struct {
	ID            string
	PromptPreview string
	Status        string
	LastLogLine   string
	Completed     bool
	Exported      string
}

// Model is the root bubbletea state.
type Model struct {
	cli       *client.Client
	connState ConnState
	sessionID *uuid.UUID

	focus    PaneID
	mode     Mode
	helpOpen bool
	input    string

	conversation []MessageRow // append-only log
	inboxEvents  []string     // pre-rendered strings
	jobs         map[string]*Job
	jobOrder     []string

	cost  CostState
	width int
	height int
}

type CostState struct {
	SessionUSD float64
	LastTurnUSD float64
	Provider   string
	Model      string
}

// New returns the initial Model. Caller must call .Run() to enter the bubbletea loop.
func New(cli *client.Client) *Model {
	return &Model{
		cli:       cli,
		connState: ConnConnecting,
		focus:     PaneConversation,
		mode:      ModeNormal,
		jobs:      make(map[string]*Job),
	}
}

// daemonEventMsg wraps a single DaemonEvent for the bubbletea Update loop.
type daemonEventMsg wire.DaemonEvent

// daemonClosedMsg fires when the events channel closes (daemon hung up).
type daemonClosedMsg struct{}

// listenForEvents returns a tea.Cmd that pulls the next event from the client.
func (m *Model) listenForEvents() tea.Cmd {
	return func() tea.Msg {
		ev, ok := <-m.cli.Events()
		if !ok {
			return daemonClosedMsg{}
		}
		return daemonEventMsg(ev)
	}
}

// Init opens a session against the current working directory.
func (m *Model) Init() tea.Cmd {
	cwd, err := os.Getwd()
	if err != nil {
		cwd = "/"
	}
	if err := m.cli.Send(wire.OpenSession(cwd)); err != nil {
		// Defer the error to a tea.Msg
		return func() tea.Msg { return daemonClosedMsg{} }
	}
	return m.listenForEvents()
}

func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		return m, nil
	case tea.KeyMsg:
		return m.rootKeybindings(msg)
	case daemonEventMsg:
		m.applyEvent(wire.DaemonEvent(msg))
		return m, m.listenForEvents()
	case daemonClosedMsg:
		m.connState = ConnDisconnected
		return m, nil
	}
	return m, nil
}

// applyEvent mutates state based on a daemon event.
func (m *Model) applyEvent(ev wire.DaemonEvent) {
	switch ev.Type {
	case "session_opened":
		m.sessionID = ev.SessionID
		m.connState = ConnConnected
	case "reply":
		m.conversation = append(m.conversation, MessageRow{Author: "arnold", Text: ev.Text})
	case "turn_complete":
		// no-op for the conversation log; status bar may pulse
	case "error":
		m.conversation = append(m.conversation, MessageRow{Author: "system", Text: "[error] " + ev.Message})
	case "job_spawned":
		if ev.JobID == nil {
			return
		}
		id := ev.JobID.String()
		m.jobs[id] = &Job{ID: id, PromptPreview: ev.PromptPreview, Status: "running"}
		m.jobOrder = append(m.jobOrder, id)
		m.inboxEvents = append(m.inboxEvents, fmt.Sprintf("job spawned: %s", id[:8]))
	case "job_update":
		if ev.JobID == nil {
			return
		}
		id := ev.JobID.String()
		if j, ok := m.jobs[id]; ok {
			j.Status = ev.Status
			if ev.LastLogLine != nil {
				j.LastLogLine = *ev.LastLogLine
			}
		}
	case "job_completed":
		if ev.JobID == nil {
			return
		}
		id := ev.JobID.String()
		if j, ok := m.jobs[id]; ok {
			j.Status = ev.Status
			j.Completed = true
			if ev.ExportedWorkspace != nil {
				j.Exported = *ev.ExportedWorkspace
			}
		}
		summary := fmt.Sprintf("job %s completed (%s)", id[:8], ev.Status)
		if ev.ExportedWorkspace != nil {
			summary += " → " + filepath.Base(*ev.ExportedWorkspace)
		}
		m.inboxEvents = append(m.inboxEvents, summary)
	case "cost_update":
		m.cost = CostState{
			SessionUSD:  ev.SessionCostUSD,
			LastTurnUSD: ev.LastTurnCostUSD,
			Provider:    ev.LastTurnProvider,
			Model:       ev.LastTurnModel,
		}
	case "notification":
		m.inboxEvents = append(m.inboxEvents, "[notify] "+ev.Title+": "+ev.Body)
	}
}

func (m *Model) sendUserMessage(text string) tea.Cmd {
	if m.sessionID == nil || strings.TrimSpace(text) == "" {
		return nil
	}
	m.conversation = append(m.conversation, MessageRow{Author: "you", Text: text})
	if err := m.cli.Send(wire.UserMessage(*m.sessionID, text)); err != nil {
		m.conversation = append(m.conversation, MessageRow{Author: "system", Text: "[error] send failed: " + err.Error()})
	}
	return nil
}

// Run starts the bubbletea program. Returns when the user quits or the daemon disconnects.
func (m *Model) Run(_ context.Context) error {
	p := tea.NewProgram(m, tea.WithAltScreen())
	_, err := p.Run()
	return err
}
```

The `View()` method is stubbed for now — Tasks 14-16 add pane renderers. Add a minimal View at the bottom of `model.go` so the model compiles:

```go
func (m *Model) View() string {
	if m.helpOpen {
		return renderHelp()
	}
	return fmt.Sprintf("arnold tui (focus=%s, mode=%d, conn=%d, sid=%v)\n%d messages | %d events | %d jobs\n[i] insert  [tab] switch pane  [?] help  [q] quit\n",
		m.focus, m.mode, m.connState, m.sessionID,
		len(m.conversation), len(m.inboxEvents), len(m.jobs),
	)
}
```

Stub `renderHelp` for now — Task 18 replaces it:

```go
func renderHelp() string {
	return "Help: see Task 18 for the real help screen\n"
}
```

- [ ] **Step 3: Update `arnold-tui/main.go` to wire the model**

Replace `arnold-tui/main.go`:

```go
package main

import (
	"context"
	"fmt"
	"os"

	"github.com/jbankse/arnold/arnold-tui/client"
	"github.com/jbankse/arnold/arnold-tui/tui"
)

func main() {
	ctx := context.Background()
	cli, err := client.Dial(ctx)
	if err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
	defer cli.Close()

	m := tui.New(cli)
	if err := m.Run(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "tui error: %v\n", err)
		os.Exit(1)
	}
}
```

- [ ] **Step 4: Verify build + minimal manual smoke**

```bash
cd /Users/joshua/Codebases/Arnold/arnold-tui
go build -o /tmp/arnold-tui-smoke ./
# Run with daemon up (you can use the v0a-installed arnoldd):
~/.local/bin/arnoldd &
sleep 1
/tmp/arnold-tui-smoke
# Press q to quit
kill %1
```

Expected: TUI opens (alt-screen), shows the placeholder text, `i` switches to insert mode, typing buffers chars, Enter sends them (you'll see "you: ..." appended to the placeholder line counter), `arnold:` replies appear, `q` quits.

- [ ] **Step 5: Commit**

```bash
cd /Users/joshua/Codebases/Arnold
git add arnold-tui/tui/model.go arnold-tui/tui/keybindings.go arnold-tui/main.go arnold-tui/go.mod arnold-tui/go.sum
git commit -m "feat(arnold-tui): root bubbletea model — events, keybindings, session lifecycle"
```

---

## Task 14: Pane renderers + status bar + help screen

**Files:**
- Create: `arnold-tui/tui/conversation.go`
- Create: `arnold-tui/tui/inbox.go`
- Create: `arnold-tui/tui/jobs.go`
- Create: `arnold-tui/tui/statusbar.go`
- Create: `arnold-tui/tui/help.go`
- Modify: `arnold-tui/tui/model.go` (replace stub View with real composition)

Layout (1-line status bar + 3 stacked panes when narrow; side-by-side when wide):

```
┌─ arnold ─────────────────────────────────────────────────────────┐
│                                                                  │
│  conversation (focused: bold border)                             │
│                                                                  │
├─ inbox ─────────────────────────────────────────────────────────┤
│  recent inbox events                                             │
├─ jobs ──────────────────────────────────────────────────────────┤
│  active 439 jobs with status indicator                          │
└─ status ────────────────────────────────────────────────────────┘
  anthropic/sonnet-4-6  •  session $0.0123  •  • conn ●  •  [conv]
```

Lipgloss handles the rendering. Each pane is a function returning a string at a given width.

- [ ] **Step 1: Write `arnold-tui/tui/conversation.go`**

```go
package tui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
)

var (
	convAuthorYou    = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("39"))   // cyan
	convAuthorArnold = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("214"))  // orange
	convAuthorSys    = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("196"))  // red
	convPrompt       = lipgloss.NewStyle().Foreground(lipgloss.Color("245"))
)

func (m *Model) renderConversation(width, height int) string {
	var b strings.Builder
	for _, row := range m.conversation {
		var label string
		switch row.Author {
		case "you":
			label = convAuthorYou.Render("you: ")
		case "arnold":
			label = convAuthorArnold.Render("arnold: ")
		default:
			label = convAuthorSys.Render(row.Author + ": ")
		}
		b.WriteString(label)
		b.WriteString(row.Text)
		b.WriteString("\n")
	}
	if m.mode == ModeInsert {
		b.WriteString(convPrompt.Render("> ") + m.input + "_")
	} else {
		b.WriteString(convPrompt.Render("> (press i to insert)"))
	}
	return paneBox("conversation", b.String(), width, height, m.focus == PaneConversation)
}
```

- [ ] **Step 2: Write `arnold-tui/tui/inbox.go`**

```go
package tui

import (
	"strings"
)

func (m *Model) renderInbox(width, height int) string {
	var b strings.Builder
	start := 0
	if len(m.inboxEvents) > 12 {
		start = len(m.inboxEvents) - 12 // keep last 12 visible
	}
	for _, ev := range m.inboxEvents[start:] {
		b.WriteString(ev)
		b.WriteString("\n")
	}
	if len(m.inboxEvents) == 0 {
		b.WriteString("(no inbox events yet)")
	}
	return paneBox("inbox", b.String(), width, height, m.focus == PaneInbox)
}
```

- [ ] **Step 3: Write `arnold-tui/tui/jobs.go`**

```go
package tui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

var (
	jobDone  = lipgloss.NewStyle().Foreground(lipgloss.Color("46"))  // green
	jobFail  = lipgloss.NewStyle().Foreground(lipgloss.Color("196")) // red
	jobRun   = lipgloss.NewStyle().Foreground(lipgloss.Color("214")) // orange
	jobQueue = lipgloss.NewStyle().Foreground(lipgloss.Color("245")) // grey
)

func statusGlyph(status string) string {
	switch status {
	case "completed":
		return jobDone.Render("✓")
	case "failed":
		return jobFail.Render("✗")
	case "running", "exporting":
		return jobRun.Render("◌")
	case "queued":
		return jobQueue.Render("…")
	case "cancelled":
		return jobQueue.Render("⊘")
	}
	return "?"
}

func (m *Model) renderJobs(width, height int) string {
	var b strings.Builder
	if len(m.jobs) == 0 {
		b.WriteString("(no 439 jobs yet — Arnold can spin one up with sys_spin_up_439)")
	} else {
		for _, id := range m.jobOrder {
			j := m.jobs[id]
			if j == nil {
				continue
			}
			short := j.ID
			if len(short) > 8 {
				short = short[:8]
			}
			line := fmt.Sprintf("%s  %s  %s", statusGlyph(j.Status), short, truncate(j.PromptPreview, 60))
			b.WriteString(line)
			if j.LastLogLine != "" && !j.Completed {
				b.WriteString("\n     " + jobQueue.Render(truncate(j.LastLogLine, 60)))
			}
			if j.Completed && j.Exported != "" {
				b.WriteString("\n     " + jobDone.Render("→ "+j.Exported))
			}
			b.WriteString("\n")
		}
	}
	return paneBox("jobs", b.String(), width, height, m.focus == PaneJobs)
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n-1] + "…"
}
```

- [ ] **Step 4: Write `arnold-tui/tui/statusbar.go`**

```go
package tui

import (
	"fmt"

	"github.com/charmbracelet/lipgloss"
)

var (
	statusBar     = lipgloss.NewStyle().Background(lipgloss.Color("236")).Foreground(lipgloss.Color("250"))
	statusOK      = lipgloss.NewStyle().Background(lipgloss.Color("236")).Foreground(lipgloss.Color("46"))
	statusBad     = lipgloss.NewStyle().Background(lipgloss.Color("236")).Foreground(lipgloss.Color("196"))
	statusBadge   = lipgloss.NewStyle().Background(lipgloss.Color("236")).Foreground(lipgloss.Color("214")).Bold(true)
)

func (m *Model) renderStatusBar(width int) string {
	connDot := statusOK.Render("●")
	connWord := "connected"
	switch m.connState {
	case ConnConnecting:
		connDot = statusBadge.Render("●")
		connWord = "connecting"
	case ConnDisconnected:
		connDot = statusBad.Render("●")
		connWord = "disconnected"
	}

	provider := m.cost.Provider
	model := m.cost.Model
	if provider == "" {
		provider = "?"
		model = "?"
	}

	left := statusBar.Render(fmt.Sprintf(" %s/%s ", provider, model))
	mid := statusBar.Render(fmt.Sprintf(" session $%.4f ", m.cost.SessionUSD))
	right := statusBar.Render(fmt.Sprintf(" %s %s ", connDot, connWord))
	focus := statusBadge.Render(fmt.Sprintf(" [%s] ", m.focus))

	// Simple horizontal compose; lipgloss handles ANSI width measurement.
	bar := lipgloss.JoinHorizontal(lipgloss.Top, left, mid, right, focus)
	if lipgloss.Width(bar) < width {
		pad := width - lipgloss.Width(bar)
		bar += statusBar.Render(string(make([]byte, pad)) /* spaces */)
	}
	return bar
}
```

Note on the padding: `make([]byte, pad)` returns zero-bytes which render as space-equivalents when wrapped in a Style.Render that backgrounds them. If that doesn't render visually correct on Joshua's terminal, replace with `strings.Repeat(" ", pad)`.

- [ ] **Step 5: Write `arnold-tui/tui/help.go`** (replaces the renderHelp stub)

```go
package tui

import "github.com/charmbracelet/lipgloss"

var helpBox = lipgloss.NewStyle().
	Border(lipgloss.RoundedBorder()).
	Padding(1, 2).
	BorderForeground(lipgloss.Color("214"))

func renderHelp() string {
	return helpBox.Render(`arnold tui — keybindings

Normal mode:
  i           insert mode (type a message in the conversation pane)
  tab         next pane
  shift+tab   previous pane
  ?           toggle this help
  q / ctrl+c  quit

Insert mode:
  esc         back to normal
  enter       send the message
  backspace   erase one char

Panes:
  conversation  you ↔ arnold
  inbox         recent events (jobs, notifications)
  jobs          in-flight 439 dispatches`)
}
```

- [ ] **Step 6: Add `paneBox` helper and rewrite `Model.View`**

Append to `arnold-tui/tui/model.go`:

```go
var (
	paneFocused   = lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).BorderForeground(lipgloss.Color("214"))
	paneUnfocused = lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).BorderForeground(lipgloss.Color("240"))
)

func paneBox(title, body string, width, height int, focused bool) string {
	style := paneUnfocused
	if focused {
		style = paneFocused
	}
	style = style.Width(width - 2).Height(height - 2)
	header := lipgloss.NewStyle().Bold(true).Render(" " + title + " ")
	return style.Render(header + "\n" + body)
}
```

Replace the placeholder `View()` with a real composition:

```go
func (m *Model) View() string {
	if m.helpOpen {
		return renderHelp()
	}
	if m.width == 0 || m.height == 0 {
		return "initializing..."
	}

	// Vertical stack: conversation (50%), inbox (20%), jobs (25%), status (1 line).
	usableH := m.height - 1 // reserve for status bar
	convH := usableH * 50 / 100
	inboxH := usableH * 20 / 100
	jobsH := usableH - convH - inboxH

	w := m.width
	return lipgloss.JoinVertical(
		lipgloss.Left,
		m.renderConversation(w, convH),
		m.renderInbox(w, inboxH),
		m.renderJobs(w, jobsH),
		m.renderStatusBar(w),
	)
}
```

Remove the old stub `View()` from earlier in the file.

- [ ] **Step 7: Build + manual smoke**

```bash
cd /Users/joshua/Codebases/Arnold/arnold-tui
go build -o /tmp/arnold-tui-smoke ./
~/.local/bin/arnoldd &
/tmp/arnold-tui-smoke
# i, type "Reply with the word OK and end your turn.", Enter
# observe arnold's reply in the conversation pane
# tab to inbox/jobs
# ? for help
# q to quit
kill %1
```

Expected: 4-pane TUI renders, conversation works as in Task 13, help opens on `?`, panes have focused-border highlighting.

- [ ] **Step 8: Commit**

```bash
cd /Users/joshua/Codebases/Arnold
git add arnold-tui/tui/conversation.go arnold-tui/tui/inbox.go arnold-tui/tui/jobs.go arnold-tui/tui/statusbar.go arnold-tui/tui/help.go arnold-tui/tui/model.go
git commit -m "feat(arnold-tui): conversation/inbox/jobs panes + status bar + help overlay"
```

---

## Task 15: Menu bar tray (`arnold-tray/`) — macOS only

**Files:**
- Create: `arnold-tray/go.mod`
- Create: `arnold-tray/main.go`
- Create: `arnold-tray/icon.go`
- Create: `arnold-tray/assets/icon_green.png`, `icon_red.png`, `icon_yellow.png`

The tray binary stays separate from arnold-tui so it can run as its own launchd-managed process and quit cleanly without taking down a TUI session. It pings the daemon over UDS every 5 seconds and updates the menu bar icon.

macOS-only in v0b. Linux libappindicator integration is deferred (Wayland + GNOME removed tray support in many versions; not worth the complexity for v0b).

- [ ] **Step 1: Initialize the Go module**

```bash
cd /Users/joshua/Codebases/Arnold/arnold-tray
go mod init github.com/jbankse/arnold/arnold-tray
go get github.com/getlantern/systray@latest
cd ..
```

- [ ] **Step 2: Create three tray icon PNGs (32x32)**

Generate small colored circle PNGs. Cheapest path: use ImageMagick `convert` if installed:

```bash
mkdir -p arnold-tray/assets
for color in green:46cc46 red:cc4646 yellow:ccbb46; do
  name="${color%%:*}"
  hex="${color##*:}"
  convert -size 32x32 xc:transparent -fill "#$hex" -draw "circle 16,16 16,6" arnold-tray/assets/icon_$name.png
done
```

If `convert` isn't available, fall back to writing three minimal hand-crafted 32x32 PNG byte literals into `icon.go` via `go:embed`. The simplest cross-platform approach: download three small dot PNGs and check them in. Either is fine — these are tiny static assets.

- [ ] **Step 3: Write `arnold-tray/icon.go`**

```go
package main

import _ "embed"

//go:embed assets/icon_green.png
var iconGreen []byte

//go:embed assets/icon_red.png
var iconRed []byte

//go:embed assets/icon_yellow.png
var iconYellow []byte
```

- [ ] **Step 4: Write `arnold-tray/main.go`**

```go
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	"github.com/getlantern/systray"
)

func main() {
	systray.Run(onReady, onExit)
}

func onReady() {
	systray.SetIcon(iconYellow)
	systray.SetTitle("")
	systray.SetTooltip("Arnold — connecting…")

	mOpen := systray.AddMenuItem("Open arnold", "Launch the arnold TUI in a new Terminal window")
	systray.AddSeparator()
	mRestart := systray.AddMenuItem("Restart daemon", "Stop and relaunch arnoldd")
	systray.AddSeparator()
	mQuit := systray.AddMenuItem("Quit arnold-tray", "Exit the menu bar app")

	// Ping loop
	go pingLoop()

	for {
		select {
		case <-mOpen.ClickedCh:
			openArnoldTUI()
		case <-mRestart.ClickedCh:
			restartDaemon()
		case <-mQuit.ClickedCh:
			systray.Quit()
			return
		}
	}
}

func onExit() {}

func pingLoop() {
	for {
		if pingDaemon(2 * time.Second) {
			systray.SetIcon(iconGreen)
			systray.SetTooltip("Arnold — daemon running")
		} else {
			systray.SetIcon(iconRed)
			systray.SetTooltip("Arnold — daemon not responding")
		}
		time.Sleep(5 * time.Second)
	}
}

func pingDaemon(timeout time.Duration) bool {
	home, err := os.UserHomeDir()
	if err != nil {
		return false
	}
	sock := filepath.Join(home, ".arnold", "arnold.sock")
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()
	d := net.Dialer{}
	conn, err := d.DialContext(ctx, "unix", sock)
	if err != nil {
		return false
	}
	defer conn.Close()
	// Send a Ping frame and expect Pong.
	if _, err := conn.Write([]byte("{\"type\":\"ping\"}\n")); err != nil {
		return false
	}
	conn.SetReadDeadline(time.Now().Add(timeout))
	buf := make([]byte, 1024)
	n, err := conn.Read(buf)
	if err != nil || n == 0 {
		return false
	}
	var ev map[string]any
	if err := json.Unmarshal(buf[:n], &ev); err != nil {
		// Maybe got more than one line; try the first line only.
		for i, b := range buf[:n] {
			if b == '\n' {
				_ = json.Unmarshal(buf[:i], &ev)
				break
			}
		}
	}
	return ev["type"] == "pong"
}

func openArnoldTUI() {
	// Use osascript to open a new Terminal window running `arnold`.
	// AppleScript: tell app "Terminal" to do script "arnold"
	script := `tell application "Terminal" to do script "arnold"`
	cmd := exec.Command("osascript", "-e", script)
	if err := cmd.Run(); err != nil {
		fmt.Fprintln(os.Stderr, "open arnold failed:", err)
	}
}

func restartDaemon() {
	// Use launchctl to unload + reload the LaunchAgent.
	home, _ := os.UserHomeDir()
	plist := filepath.Join(home, "Library", "LaunchAgents", "com.arnold.arnoldd.plist")
	_ = exec.Command("launchctl", "unload", plist).Run()
	_ = exec.Command("launchctl", "load", plist).Run()
}
```

- [ ] **Step 5: Build + smoke**

```bash
cd /Users/joshua/Codebases/Arnold/arnold-tray
go build -o /tmp/arnold-tray-smoke ./
/tmp/arnold-tray-smoke &
# Look at the macOS menu bar — a yellow dot appears (then green after 5s if daemon is up).
# Click → menu items show.
# Click "Quit arnold-tray" to exit.
```

Expected: menu bar icon appears, ping loop transitions to green, menu items work.

- [ ] **Step 6: Commit**

```bash
cd /Users/joshua/Codebases/Arnold
git add arnold-tray/go.mod arnold-tray/go.sum arnold-tray/main.go arnold-tray/icon.go arnold-tray/assets/
git commit -m "feat(arnold-tray): macOS menu bar status icon — ping loop, restart, open arnold"
```

---

## Task 16: macOS LaunchAgent + Linux systemd unit + install/uninstall script updates

**Files:**
- Create: `scripts/arnoldd.plist.template`
- Create: `scripts/arnold-tray.plist.template`
- Create: `scripts/arnoldd.service.template`
- Create: `scripts/build-bios.sh`
- Create: `scripts/build-tui.sh`
- Create: `scripts/build-tray.sh`
- Modify: `scripts/install.sh`
- Modify: `scripts/uninstall.sh`

- [ ] **Step 1: Write `scripts/arnoldd.plist.template`** (macOS LaunchAgent, copied with `__BIN_DIR__` placeholder substitution by install.sh)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.arnold.arnoldd</string>
    <key>ProgramArguments</key>
    <array>
        <string>__BIN_DIR__/arnoldd</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>StandardOutPath</key>
    <string>__ARNOLD_DIR__/arnoldd.log</string>
    <key>StandardErrorPath</key>
    <string>__ARNOLD_DIR__/arnoldd.err</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>__USER_PATH__</string>
    </dict>
</dict>
</plist>
```

- [ ] **Step 2: Write `scripts/arnold-tray.plist.template`**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.arnold.arnold-tray</string>
    <key>ProgramArguments</key>
    <array>
        <string>__BIN_DIR__/arnold-tray</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>__ARNOLD_DIR__/arnold-tray.log</string>
    <key>StandardErrorPath</key>
    <string>__ARNOLD_DIR__/arnold-tray.err</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>__USER_PATH__</string>
    </dict>
</dict>
</plist>
```

- [ ] **Step 3: Write `scripts/arnoldd.service.template`** (Linux systemd user unit)

```ini
[Unit]
Description=Arnold personal-agent daemon
After=network.target

[Service]
ExecStart=__BIN_DIR__/arnoldd
Restart=on-failure
StandardOutput=append:__ARNOLD_DIR__/arnoldd.log
StandardError=append:__ARNOLD_DIR__/arnoldd.err

[Install]
WantedBy=default.target
```

- [ ] **Step 4: Write `scripts/build-bios.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
VENDOR="$ROOT/vendor/439"
DEST_DIR="${1:-$HOME/.arnold/bin}"

mkdir -p "$DEST_DIR"
cd "$VENDOR"
cargo build --release -p bios
cp target/release/bios "$DEST_DIR/bios"
echo "Installed bios binary to $DEST_DIR/bios"
```

Make executable: `chmod +x scripts/build-bios.sh`.

- [ ] **Step 5: Write `scripts/build-tui.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"

cd "$ROOT/arnold-tui"
go build -o "$BIN_DIR/arnold" ./
echo "Installed arnold TUI to $BIN_DIR/arnold"
```

Make executable.

- [ ] **Step 6: Write `scripts/build-tray.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"

if [[ "$(uname)" != "Darwin" ]]; then
    echo "arnold-tray is macOS-only in v0b; skipping on $(uname)"
    exit 0
fi

cd "$ROOT/arnold-tray"
go build -o "$BIN_DIR/arnold-tray" ./
echo "Installed arnold-tray to $BIN_DIR/arnold-tray"
```

Make executable.

- [ ] **Step 7: Rewrite `scripts/install.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"
ARNOLD_DIR="${ARNOLD_DIR:-$HOME/.arnold}"

OS="$(uname)"

echo "[1/8] building 439 cpu binary..."
"$HERE/build-cpu.sh" "$ARNOLD_DIR/bin"

echo "[2/8] building 439 bios binary..."
"$HERE/build-bios.sh" "$ARNOLD_DIR/bin"

echo "[3/8] building arnoldd..."
cd "$ROOT"
cargo build --release -p arnoldd

echo "[4/8] building arnold TUI..."
ARNOLD_BIN_DIR="$BIN_DIR" "$HERE/build-tui.sh"

echo "[5/8] building arnold-tray (macOS only)..."
ARNOLD_BIN_DIR="$BIN_DIR" "$HERE/build-tray.sh"

echo "[6/8] installing arnoldd binary..."
mkdir -p "$BIN_DIR" "$ARNOLD_DIR/workspace" "$ARNOLD_DIR/memory" "$ARNOLD_DIR/jobs"
install -m 755 "$ROOT/target/release/arnoldd" "$BIN_DIR/arnoldd"

echo "[7/8] writing default config (if absent)..."
if [ ! -f "$ARNOLD_DIR/config.toml" ]; then
  cat > "$ARNOLD_DIR/config.toml" <<EOF
cpu_binary = "$ARNOLD_DIR/bin/cpu"
bios_binary = "$ARNOLD_DIR/bin/bios"
runtime_image = "runtime/os:local"
llm_provider = "anthropic"
model = "claude-sonnet-4-6"
allowed_dirs = []
EOF
fi

echo "[8/8] installing autostart service..."
if [[ "$OS" == "Darwin" ]]; then
  AGENTS_DIR="$HOME/Library/LaunchAgents"
  mkdir -p "$AGENTS_DIR"

  for unit in arnoldd arnold-tray; do
    SRC="$HERE/${unit}.plist.template"
    DST="$AGENTS_DIR/com.arnold.${unit}.plist"
    sed -e "s|__BIN_DIR__|$BIN_DIR|g" \
        -e "s|__ARNOLD_DIR__|$ARNOLD_DIR|g" \
        -e "s|__USER_PATH__|$PATH|g" \
        "$SRC" > "$DST"
    launchctl unload "$DST" 2>/dev/null || true
    launchctl load "$DST"
  done

  echo "Loaded com.arnold.arnoldd + com.arnold.arnold-tray via launchctl."
elif [[ "$OS" == "Linux" ]]; then
  UNIT_DIR="$HOME/.config/systemd/user"
  mkdir -p "$UNIT_DIR"
  SRC="$HERE/arnoldd.service.template"
  DST="$UNIT_DIR/arnoldd.service"
  sed -e "s|__BIN_DIR__|$BIN_DIR|g" \
      -e "s|__ARNOLD_DIR__|$ARNOLD_DIR|g" \
      "$SRC" > "$DST"
  systemctl --user daemon-reload
  systemctl --user enable --now arnoldd.service

  echo "Enabled arnoldd.service via systemctl --user."
else
  echo "WARNING: autostart not supported on $OS; you'll need to run arnoldd manually."
fi

echo
echo "Installed. arnoldd is autostarting; arnold (TUI) is available at:"
echo "  $BIN_DIR/arnold"
if [[ "$OS" == "Darwin" ]]; then
  echo "Menu bar icon should appear within a few seconds."
fi
```

Make executable.

- [ ] **Step 8: Rewrite `scripts/uninstall.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
BIN_DIR="${ARNOLD_BIN_DIR:-$HOME/.local/bin}"
ARNOLD_DIR="${ARNOLD_DIR:-$HOME/.arnold}"
OS="$(uname)"

if [[ "$OS" == "Darwin" ]]; then
  for unit in arnoldd arnold-tray; do
    PLIST="$HOME/Library/LaunchAgents/com.arnold.${unit}.plist"
    if [ -f "$PLIST" ]; then
      launchctl unload "$PLIST" 2>/dev/null || true
      rm -f "$PLIST"
    fi
  done
elif [[ "$OS" == "Linux" ]]; then
  systemctl --user disable --now arnoldd.service 2>/dev/null || true
  rm -f "$HOME/.config/systemd/user/arnoldd.service"
  systemctl --user daemon-reload || true
fi

rm -f "$BIN_DIR/arnoldd" "$BIN_DIR/arnold" "$BIN_DIR/arnold-tray"
echo "Removed binaries + autostart units. To remove state, manually delete $ARNOLD_DIR"
```

Make executable.

- [ ] **Step 9: Commit**

```bash
chmod +x scripts/install.sh scripts/uninstall.sh scripts/build-bios.sh scripts/build-tui.sh scripts/build-tray.sh
git add scripts/
git commit -m "feat(scripts): launchd/systemd autostart, separate build scripts, install orchestrator"
```

---

## Task 17: Retire `arnold-cli` Rust crate

**Files:**
- Delete: `arnold-cli/`
- Modify: `Cargo.toml` (remove from workspace members)
- Modify: `arnoldd/src/main.rs` (no change actually needed; just confirming nothing references it)

The Go TUI fully replaces the Rust REPL. Retire cleanly.

- [ ] **Step 1: Confirm nothing in the Rust workspace depends on `arnold-cli`**

Run: `grep -rn "arnold-cli\|arnold_cli" --include="*.rs" --include="*.toml" -- arnoldd/ arnold-wire/ Cargo.toml 2>/dev/null`

Expected: only matches in the root `Cargo.toml`'s workspace members list. Nothing else should reference it.

- [ ] **Step 2: Remove `arnold-cli` from workspace members**

Edit `Cargo.toml`. Change:

```toml
[workspace]
resolver = "2"
members = ["arnold-wire", "arnoldd", "arnold-cli"]
```

to:

```toml
[workspace]
resolver = "2"
members = ["arnold-wire", "arnoldd"]
```

- [ ] **Step 3: Delete the crate directory**

```bash
git rm -r arnold-cli/
```

- [ ] **Step 4: Verify build**

Run: `cargo build --workspace 2>&1 | tail -5`

Expected: clean compile, only `arnold-wire` and `arnoldd` built.

Run: `cargo test --workspace 2>&1 | tail -5`

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml
git commit -m "chore: retire arnold-cli Rust REPL — replaced by Go TUI in v0b"
```

---

## Task 18: README + CLAUDE.md updates for v0b

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Replace `README.md`'s Quickstart section**

Open `README.md`. The v0a Quickstart appended in Task 17 of the v0a plan currently looks like:

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

Replace the entire Quickstart section with:

```markdown
## Quickstart (v0b)

```bash
./scripts/install.sh                      # builds cpu + bios + arnoldd + Go TUI (+ tray on macOS); installs autostart
export ANTHROPIC_API_KEY=sk-...           # daemon reads this at startup
# On macOS, arnoldd + arnold-tray autostart at login (loaded via launchctl).
# On Linux, arnoldd autostarts via `systemctl --user enable`.
arnold                                    # open the bubbletea TUI in your terminal
```

Inside the TUI:
- Press `i` to enter insert mode, type a message, Enter to send
- `tab` / `shift+tab` cycle between conversation / inbox / jobs panes
- `?` shows the keybindings help overlay
- `q` quits

Arnold's v0b capabilities:
- Talk to a coding-focused agent backed by an LLM provider (Anthropic by default)
- Read / write / search files, run commands, fetch web pages, persist memory
- Spin up ephemeral [439](https://github.com/jbankse/439) environments for build/refactor/test tasks via `sys_spin_up_439` (returns immediately; completion arrives as an inbox event)
- Receive OS notifications when long-running work finishes

For a pre-flight health check without spending an LLM turn:

```bash
arnoldd --check
```
```

- [ ] **Step 2: Add a one-paragraph note to `CLAUDE.md` about the Go modules**

Append to `CLAUDE.md`:

```markdown

## Toolchain note (v0b+)

The repo now contains two Go modules (`arnold-tui/` and `arnold-tray/`) alongside the Cargo workspace (`arnold-wire`, `arnoldd`). The Go modules are siblings — they don't share Go workspace state and don't link against the Cargo workspace. Build them with `go build ./arnold-tui/...` and `go build ./arnold-tray/...` respectively; their install paths are handled by `scripts/build-tui.sh` / `scripts/build-tray.sh`.

When changing wire types, edit `arnold-wire/src/lib.rs` AND `arnold-tui/wire/wire.go` together — the Go file is a hand-maintained mirror, and the JSON shape must stay byte-identical between the two. A common bug pattern is to add a Rust variant without updating the Go struct; the daemon will emit it, the TUI will silently ignore it.
```

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: v0b Quickstart in README; note Go-modules-alongside-Cargo in CLAUDE.md"
```

---

## Task 19: End-to-end v0b smoke test

**Files:**
- None modified

This task is a manual integration check. It does NOT add code. The implementer (or Joshua) runs it after Tasks 1-18 land to verify v0b actually works.

- [ ] **Step 1: Clean install**

```bash
~/.local/bin/arnoldd-stop 2>/dev/null || true
launchctl unload ~/Library/LaunchAgents/com.arnold.arnoldd.plist 2>/dev/null || true
launchctl unload ~/Library/LaunchAgents/com.arnold.arnold-tray.plist 2>/dev/null || true
rm -f ~/.arnold/arnold.sock
./scripts/install.sh
```

Expected:
- cpu + bios binaries built (10-30 min cold)
- arnoldd built clean
- arnold + arnold-tray Go binaries built clean
- launchctl loads both plists
- macOS menu bar shows a yellow→green dot within 10s

- [ ] **Step 2: Pre-flight check**

```bash
arnoldd --check
```

Expected: prints `arnoldd --check OK: cpu finished` and exits 0.

If it fails: read the error, fix the config (most likely `ANTHROPIC_API_KEY` unset or `cpu_binary` path wrong), repeat.

- [ ] **Step 3: First TUI session**

```bash
arnold
```

In the TUI:
1. Verify all four panes render (conversation, inbox, jobs, status bar)
2. Verify status bar shows the connected dot + provider/model
3. Press `i`, type `Reply with the word OK and end your turn.`, Enter
4. Verify "arnold: OK" appears in the conversation pane
5. Verify status bar updates the session cost to a non-zero value (~$0.0001 for a Sonnet call)
6. Press `?` to verify help overlay appears
7. Press `q` to quit

- [ ] **Step 4: Test 439 dispatch**

Back in the TUI:

```
i Please use sys_spin_up_439 to build a Rust CLI called "hello" that prints "Hello World", then sys_done. Enter
```

Expected over the next 1-5 minutes:
1. Conversation pane shows "arnold: spinning up 439..." (or similar) and the syscall result with a job_id
2. Jobs pane shows a new row with an orange "◌ running" indicator
3. Inbox pane logs "job spawned: <8-char-id>"
4. The bios subprocess starts (visible if `docker ps` shows the runtime container)
5. After bios completes, jobs pane row turns green "✓ completed" with the exported workspace path
6. Inbox pane shows "job <id> completed (completed) → workspace"
7. Status bar's session cost has updated
8. On macOS: a notification banner pops up (if Arnold called sys_send_notification)

If 439 dispatch is being problematic (no docker, image not built), this is the test that surfaces it. Investigate from the daemon's `~/.arnold/arnoldd.log` and bios's stderr captured in the JobCompleted message.

- [ ] **Step 5: Test menu bar (macOS only)**

1. Click the green dot in the menu bar
2. Click "Open arnold" → a new Terminal window opens running the TUI
3. Click "Restart daemon" → daemon restarts (you'll see the menu bar icon flicker yellow then green)
4. Don't click "Quit arnold-tray" unless you want the tray gone (launchctl will respawn it next login)

- [ ] **Step 6: Test crash recovery**

```bash
pkill arnoldd
sleep 6      # launchd should respawn it (KeepAlive on non-success)
arnoldd --check
```

Expected: `--check` passes within 10 seconds of the kill, confirming launchd respawned.

- [ ] **Step 7: Commit (only if you made any fixes during smoke; otherwise no commit)**

If smoke surfaces issues, fix them as separate `fix(...)` commits per the v0a pattern (kill_on_drop, byte/char, etc.). Don't bundle multiple fixes into one commit — keep history bisect-friendly.

---

## Self-Review (run BEFORE handing the plan to subagent-driven-development)

After writing all 19 tasks, the plan author runs through this checklist to catch gaps:

**Spec coverage check** — every line of the v0 spec's v0b scope must map to a task:

| Spec item | Task | Notes |
|---|---|---|
| `sys_spin_up_439` native async syscall | Task 7 (enum) + Task 8 (handler) | ✓ |
| `sys_poll_job` | Task 7 + Task 8 | ✓ |
| In-flight job table | Task 2 | ✓ |
| BIOS subprocess management | Task 6 | ✓ |
| Workspace artifact extraction to `~/.arnold/jobs/<job_id>/` | Task 6 (export_path in spawn_and_drive) | ✓ |
| `sys_send_notification` + OS notifications | Task 7 + Task 9 | ✓ |
| Go TUI client (bubbletea/bubbles) | Tasks 11-14 | ✓ |
| Multi-pane TUI: conversation + inbox + 439 jobs + status bar with cost meter | Task 14 | ✓ |
| Retire interim Rust CLI | Task 17 | ✓ |
| launchd/systemd unit | Task 16 | ✓ |

**v0b plan extras beyond spec:**
- `arnoldd --check` pre-flight (Task 5) — Opus deferral from v0a
- Friendlier `rg`-missing diagnostic (Task 5) — Opus deferral from v0a
- Menu bar status icon (Task 15) — Joshua direction during plan brainstorm

**Open questions resolved:**
- cpu binary versioning: confirmed `vendor/439` submodule pinned to same tag for cpu + bios (Task 6 + install.sh)
- Workspace export contract: BIOS exit code is the source of truth; no verdict JSON to parse (Task 6 derived from `vendor/439/bios/src/runner.rs::run`)
- launchd vs lazy spawn: launchd + systemd user services chosen (Task 16)
- TUI ↔ daemon wire schema: arnold-wire extended in Task 1, Go mirror in Task 11
- Cost meter source: cpu's `usage` up-frame intercepted in Task 3 (promoted from `Other` to typed `Usage` variant), accumulated by `UsageMeter`, emitted as `DaemonEvent::CostUpdate`
- Default jail policy UX: manual config edit + daemon restart (Task 16's install.sh writes config; user edits `~/.arnold/config.toml` and re-runs `launchctl unload && launchctl load` or `systemctl --user restart arnoldd`)

**Placeholder scan:** none — every code block contains real Rust / Go / bash. No "TBD", no "add error handling", no "similar to Task N".

**Type consistency:**
- `Syscall::SpinUp439` fields: `prompt: String, pack_kind: Option<String>, export_workspace_to: Option<String>, env: Option<HashMap<String, String>>` — consistent across Task 7's definition, Task 8's destructuring, Task 10's syscall menu entry
- `JobStatus` enum: 6 variants (Queued/Running/Exporting/Completed/Failed/Cancelled) — consistent across Task 2 (definition) and Task 6 (status transitions in bios driver) and Task 8 (return on dispatcher error)
- Wire types: arnold-wire's `DaemonEvent::JobCompleted { exit_code: Option<i32>, exported_workspace: Option<String> }` — matches Go's `DaemonEvent.ExitCode *int, ExportedWorkspace *string` in Task 11

**File-structure sanity:**
- All file paths in tasks match the File Structure section at the top
- New deps declared in workspace `Cargo.toml` (Task 6: `which`; Task 9: `notify-rust` reserved) before per-crate Cargo.toml additions

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-21-arnold-v0b-439-dispatch-and-tui.md`. Three considerations before kicking off subagent-driven execution:

1. **Branch strategy.** v0a chose direct-on-main. v0b is bigger (~19 tasks, fold-in fixes for the cpu-orphan and byte/char patterns expected mid-execution). Joshua may prefer a `feat/v0b` branch this time so a mid-stream "let's revisit Task 8's BIOS contract" can be amended cleanly without polluting main.
2. **Subagent model selection.** Per v0a's experience: Sonnet 4.6 for implementers; Opus 4.7 for code-quality reviewers (caught two security bypasses, the schema normalizer no-op, and the cpu-orphan class). Spec reviewers can stay on Sonnet.
3. **Smoke-test gating.** Task 19 is manual and needs a real `ANTHROPIC_API_KEY` + a working docker daemon for the BIOS run. The plan can be executed Tasks 1-18 in CI-style headless mode, then Task 19 deferred to whenever Joshua has the time + keys to run it.

Recommended next step: confirm branch strategy, then invoke `superpowers:subagent-driven-development` to dispatch Task 1.

**v0b will be done when Task 19's smoke succeeds end-to-end.**
