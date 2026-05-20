# Arnold — Personal-Agent Design (v0)

**Status:** v0 design, ready for implementation planning.
**Date:** 2026-05-20
**Repo:** `/Users/joshua/Codebases/Arnold` (new, this is the first committed doc).
**Author:** Joshua Banks (with brainstorm assistance).

---

## Summary

Arnold is a long-running, general-purpose, single-agent personal assistant that lives as a persistent daemon on the user's machine. Its v0 form is a **code-focused daily-driver** (Claude-Code-adjacent), with one signature capability over comparable tools: a native, asynchronous `sys_spin_up_439` syscall that lets Arnold dispatch ephemeral [439](https://github.com/jbankse/439) environments for any domain pack while remaining responsive to ongoing conversation. As 439's pack ecosystem grows, so does Arnold's toolbelt at zero Arnold-side cost.

Arnold is a separate product from 439, in a separate repository, consuming 439 as a dependency. 439 is not modified for Arnold's sake.

---

## Vision: Two Products, Two Categories

| | **439** | **Arnold** |
|---|---|---|
| Identity | Agent-environment runtime + pack SDK | A named singular agent |
| Customer | Developers building agentic systems on top of a hardened decomposition substrate | End users who want *an agent*, not a runtime |
| Shape | Ephemeral, multi-role (planner→workers→verifiers), jailed-per-task | Persistent, single-agent loop, long-lived environment |
| Repo | `/Users/joshua/Codebases/439` (unchanged for Arnold) | `/Users/joshua/Codebases/Arnold` (this repo) |
| Lifecycle | Spawn per task, run to completion, export, discard | Long-running daemon; per-session conversation history |
| Relationship | Stays a black-box dependency of Arnold | Consumes 439 via vendored submodule + `bios` subprocess + `cpu`/`providers` lib reuse |

The two products complement on the axis of **clean-room vs. persistent**, not coding vs. non-coding. Arnold handles anything that doesn't need a clean room; 439 handles anything that does, via a pack-defined environment.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  User's machine                                                  │
│                                                                  │
│  $ arnold ◀── Go TUI client (bubbletea/bubbles) ────────┐        │
│        talks to daemon over UDS, JSON-line frames       │        │
│                                                         │        │
│  ┌──── arnoldd: Rust daemon (persistent host process) ◀─┘        │
│  │                                                               │
│  │  Single-agent loop                                            │
│  │  ├─ Inbox (SQLite): user msgs, 439 completions,               │
│  │  │   scheduled triggers, client connect/disconnect            │
│  │  ├─ In-flight job table: live background 439 envs             │
│  │  ├─ Syscall harness (jail + frame protocol patterned on 439)  │
│  │  ├─ Markdown memory at ~/.arnold/memory/                      │
│  │  └─ Spawns `cpu` subprocess per session                       │
│  │     (reuses 439's cpu binary + providers C++ lib)             │
│  │                                                               │
│  └────────────────────┬──────────────────────────────────────────┘
│                       │ sys_spin_up_439 → fork bios               
│                       ▼                                            
│  ┌── Ephemeral 439 environment (local Docker today) ──────┐      │
│  │   bios + os + cpu + active domain pack                 │      │
│  │   jailed workspace, runs to completion, exports        │      │
│  │   artifacts → ~/.arnold/jobs/<job_id>/workspace/       │      │
│  │   exit → job_completed inbox event                     │      │
│  └────────────────────────────────────────────────────────┘      │
└──────────────────────────────────────────────────────────────────┘
```

### Core Principles

- **Arnold is just an agent.** Single normal-agent loop with a syscall-shaped harness borrowed from 439's protocol/frame discipline. **Not** the multi-role planner/developer/verifier decomposition — that's 439's machinery and stays in 439.
- **`sys_spin_up_439` is asynchronous.** Returns a `job_id` immediately; the 439 run executes in its own ephemeral container; completion lands as an inbox event when ready. Arnold's loop stays responsive throughout — the user can converse, Arnold can do other syscalls, multiple jobs can run concurrently.
- **439 stays unchanged.** Arnold invokes the existing `bios` binary as a subprocess. The `ContainerRuntime` trait on 439's side is the seam where local-Docker → cloud impl gets swapped later, with no Arnold-side changes required.
- **Continuity = persistent daemon + filesystem.** Arnold's `~/.arnold/` directory persists across sessions: memory (markdown), inbox (SQLite), in-flight jobs, environmental state. The daemon itself is long-lived (launchd/systemd unit); closing the CLI just disconnects the client.
- **Polyglot stack with clean process boundaries:** Rust daemon + C++ (`cpu` + `providers` reused from 439) + Go TUI client. Each boundary is a process, not in-process linking, so polyglot cost stays contained.
- **v0 simplification:** Arnold himself is **not yet containerized** in v0. He's a host-side persistent daemon. His `~/.arnold/` on the user's filesystem provides persistence. Containerizing Arnold himself is a v1 step for distribution/isolation. This avoids Docker-in-Docker (Arnold-in-container spawning 439-in-container) for v0.

---

## v0 Scope

### In scope

- Persistent host daemon (`arnoldd`), Rust binary, started by launchd (macOS) / systemd (Linux) on login.
- Go TUI client (`arnold`) using bubbletea + bubbles. Multi-pane: active conversation + inbox + in-flight 439 jobs + status bar with cost meter.
- UDS at `~/.arnold/arnold.sock` (mode 0600), JSON-line wire protocol between client and daemon.
- Single-agent loop with the v0 syscall harness defined below — jail, frame protocol, validation patterned on 439.
- Markdown memory store at `~/.arnold/memory/` with `MEMORY.md` index + topic files (taxonomy mirrors Claude Code's auto-memory: `user`, `feedback`, `project`, `reference`).
- Inbox (SQLite-backed) for: user messages, 439 completion events, scheduled triggers, client connect/disconnect.
- In-flight job table tracking live background 439 envs.
- `sys_spin_up_439` native async syscall, completion-event delivery, workspace artifact extraction to `~/.arnold/jobs/<job_id>/`.
- OS notifications via macOS Notification Center / Linux `notify-send` when the CLI client is disconnected and a job completes.
- Provider/model integration by reusing 439's `providers` C++ library + `cpu` binary (zero new provider work).
- Default model: Claude Sonnet 4.6, configurable per session and via env (`ARNOLD_MODEL`, `ARNOLD_PROVIDER`).
- Install/uninstall scripts: build all binaries + the 439 runtime Docker image, install launchd/systemd unit, wire PATH.

### Explicitly deferred (post-v0)

| Item | Target | Why deferred |
|---|---|---|
| Containerizing Arnold himself | v1.5 | Needed when distributing beyond the v0 author; v0 has one user |
| Cloud 439 backend | v2 | New `ContainerRuntime` impl in 439; orthogonal to Arnold v0 |
| Hybrid memory (markdown + vector / FTS) | v1 | Markdown corpus stays manageable at v0 user scale |
| Maelstrom executor (`sys_run_maelstrom`) | v1 | Maelstrom is design-phase; v0 Arnold knows syntax for write-only synthesis |
| Broader personal-agent syscalls (calendar, mail, browser, OS apps) | v1+ | Gated by which use cases prove out in v0 |
| `sys_web_search` | v0.1 | Quick add; need to pick provider (Brave / Tavily / Exa) |
| `sys_schedule` (cron-like) | v0.1 | Needs a small scheduler in the daemon |
| Persona / named character system prompt | v1 if it matters for brand | v0 ships generic "helpful code-focused personal agent" |
| Multi-conversation / tmux-style sessions | v1+ | Only if single-session proves limiting |
| Web UI / mobile clients | v2 | Coincides with cloud |
| Cost caps & quotas (beyond reusing 439's `AGENT_RUN_MAX_*`) | v1 | v0 borrows 439's patterns; full budgeting later |

---

## v0 Syscall Surface

Borrowing 439's framing/jail discipline. Each call validated against a jail (`~/.arnold/workspace/` + user-allowed dirs declared in config) before any side effect.

### Conversation lifecycle

- `sys_reply { text }` — Arnold's primary user-facing output. Routed to the connected CLI client; buffered to inbox if no client connected.
- `sys_done` — end the current turn; agent loop waits on the inbox for the next event.

### File / project

- `sys_read_file { path }`
- `sys_write_file { path, contents }`
- `sys_replace_in_file { path, old_string, new_string }` (borrowed from 439)
- `sys_list_dir { path }`
- `sys_search { query, path?, glob? }` — ripgrep wrapper

### Shell

- `sys_run_command { cmd, args?, cwd?, timeout_ms?, stdin? }` — bounded, captured output, no shell interpolation

### Web

- `sys_web_fetch { url }` — returns text/markdown

### Memory

- `sys_memory_read { topic }`
- `sys_memory_write { topic, content }` — also updates `MEMORY.md` index
- `sys_memory_list` — returns `MEMORY.md` verbatim

### 439 dispatch

- `sys_spin_up_439 { prompt, pack_kind?, export_workspace_to?, env? }` — async; returns `{ job_id }`. Daemon spawns `bios run ... -- "<prompt>"` in the background.
- `sys_poll_job { job_id }` — returns `{ status, started_at, recent_log_tail }`. Status: `queued | running | completed | failed | cancelled`.

### Notifications

- `sys_send_notification { title, body }` — OS-level surface; used proactively by Arnold to notify on background completions when client is disconnected.

That's ~13 syscalls. `sys_web_search`, `sys_schedule`, `sys_cancel_job`, `sys_log`, `sys_delete_file`, `sys_create_dir` are deliberate v0.1 adds after observing the v0 set in use.

---

## 439 Dependency Model

### Subprocess + library hybrid

- **(a) Subprocess via `bios` binary** — for actual 439 dispatch. Arnold shells out: `bios run --provider docker --image runtime/os:local --export-workspace <path> -- "<prompt>"`. 439 stays a black box; Arnold only knows the BIOS CLI surface (stable) and the workspace export contract. Cleanest separation; zero risk of breaking 439; easy to update either side independently.
- **(b) Library reuse for `cpu` + `providers`** — Arnold's daemon spawns 439's existing `cpu` binary per session (the same way 439's `os` does), passing plan frames carrying Arnold's role definition, system prompt, syscall menu, and `constraint_schema`. The `cpu` binary statically links 439's `providers` C++ library. Arnold reuses all of: seven provider adapters, retry, native syscall tool transport, prompt caching, reasoning controls, cost model, L2 archival. This avoids reimplementing polished provider code.
- **Arnold defines its own role** (e.g., `assistant`) with its own `ROLE.md` body and syscall menu (the v0 list above). Arnold does not reuse 439's planner/developer/verifier/brainstormer/debugger roles.

### Concrete v0 layout

```
Codebases/Arnold/
├── .git/
├── vendor/
│   └── 439/                   # git submodule pinned to a 439 release tag
├── arnoldd/                   # Rust binary, the persistent daemon
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── inbox.rs
│       ├── jobs.rs
│       ├── memory.rs
│       ├── syscall_handler.rs
│       ├── jail.rs
│       ├── cpu_process.rs     # spawns 439's cpu binary, frames protocol
│       ├── bios_spawn.rs      # shells out to 439's bios
│       └── uds_server.rs      # client wire protocol
├── arnold-cli/                # Go binary, the TUI client
│   ├── go.mod
│   ├── main.go
│   ├── tui/                   # bubbletea/bubbles models
│   ├── wire/                  # JSON-line frame types matching arnoldd
│   └── notify/                # OS notification helpers
├── role/
│   ├── ROLE.md                # Arnold's system-prompt body
│   └── examples/              # worked examples for Arnold's role
├── docs/superpowers/
│   ├── specs/                 # this file lives here
│   ├── plans/                 # implementation plans
│   └── handoffs/              # dated handoff docs
├── scripts/
│   ├── install.sh             # builds 439 (bios + cpu + runtime image),
│   │                          # builds arnoldd + arnold-cli, installs unit
│   └── uninstall.sh
├── README.md
├── CLAUDE.md
└── AGENTS.md
```

The "fork or dep" choice becomes "which 439 tag does `vendor/439` point at" — easy to bump, pin, or roll back.

---

## Memory

Pattern: same as Claude Code's auto-memory system (which the author uses daily and trusts).

- `~/.arnold/memory/MEMORY.md` — single index file, ~150-char bullets pointing at topic files. Always loaded into Arnold's context at session start.
- `~/.arnold/memory/<topic>.md` — individual topic files with frontmatter (`name`, `description`, `type`, `last_updated`).
- Types: `user` (about the user — role, preferences, knowledge), `feedback` (corrections/validations Arnold should not re-learn), `project` (per-project facts, decisions, ongoing work), `reference` (pointers to external systems).
- Per-project notes at `~/.arnold/memory/projects/<project_slug>.md`, loaded when Arnold's active session is in that project's directory.
- `sys_memory_read(topic)` returns the topic file. `sys_memory_write(topic, content)` creates/updates the file AND updates `MEMORY.md`. `sys_memory_list` returns `MEMORY.md` verbatim.
- Git-versionable if user wants — Arnold doesn't manage git for it; layout supports it.

v1 hybrid (vector + FTS under the same markdown corpus) is tracked as a separate project memory in this codebase.

---

## User Surface

### Daemon (`arnoldd`)

- Persistent Rust binary, started by launchd (macOS) / systemd (Linux) on login.
- Listens on `~/.arnold/arnold.sock` (UDS, mode 0600).
- Owns: inbox SQLite at `~/.arnold/inbox.db`, in-flight job table, per-session conversation history in memory, `cpu` subprocesses, `bios` subprocesses.

### TUI client (`arnold`)

- Lightweight Go binary on PATH, built with [bubbletea](https://github.com/charmbracelet/bubbletea) + [bubbles](https://github.com/charmbracelet/bubbles).
- Connects to the daemon socket.
- Multi-pane TUI:
  - Main pane: active conversation (markdown-rendered).
  - Sidebar: inbox + in-flight 439 jobs (live status, current syscall, elapsed, cost-so-far).
  - Status bar: model, session id, daemon health, total session cost.
- Subcommands:
  - `arnold` — open interactive TUI session against current working directory's project
  - `arnold inbox` — list pending events
  - `arnold jobs` — list active background 439 envs
  - `arnold status` — daemon health
  - `arnold stop` — graceful daemon stop

### Session model

- One session per `arnold` invocation. Closing the client disconnects, but session history persists in the daemon. Reconnecting picks up where you left off — `/resume`-style is the default, not opt-in.
- Background events that land while no client is connected (e.g., a 439 build finishing at 3am) buffer in the inbox AND fire an OS notification.

---

## Concurrency / Event Loop

- Arnold's turn is reactive: each turn = daemon presents Arnold's `cpu` with the active session context + any pending inbox events; Arnold reacts via one syscall (native tool); cycle repeats until `sys_done` ends the turn and the loop waits on the inbox.
- `sys_spin_up_439` does not block. Returns `{ job_id }` immediately. Daemon spawns `bios run ...` in a child process; tracks `{ job_id, kind, prompt, container_id, status, started_at, export_path }` in the in-flight job table.
- When a 439 child exits, the daemon parses BIOS' output (verdict, exit code), copies the exported workspace to `~/.arnold/jobs/<job_id>/workspace/`, and emits `inbox_event { kind: "job_completed", job_id, verdict, workspace_path }`.
- If a client is connected, Arnold's next turn includes the event in context. If not, the event sits in the inbox AND triggers `sys_send_notification`.

### Inbox event types (v0)

- `user_message { text }`
- `client_connected { client_id }` / `client_disconnected { client_id }`
- `job_completed { job_id, verdict, workspace_path }`
- `job_failed { job_id, reason, log_tail }`

### Concurrency cap

Configurable max concurrent 439 envs (default 3). Excess `sys_spin_up_439` calls return `{ job_id, status: "queued" }`. The daemon promotes queued jobs to `running` as slots free up.

---

## Maelstrom Integration (v1 planned, v0 forward-compat only)

[Maelstrom](https://github.com/jbankse/maelstrom) is a clean-slate scripting language for safe automation, currently in design phase (no implementation yet). It is the natural substrate for a powerful Arnold capability: **"save a multi-step operation as a reusable typed script."**

### Use case

When Arnold performs a multi-step operation worth keeping (a deploy checklist, a project bootstrap, a weekly digest), instead of re-running it through LLM-driven syscalls every time, he writes a `.mael` file the user owns, audits, edits, and re-runs cheaply. The `needs:` capability block makes safety review trivial; Whirlpooling makes composition sound.

### v0 posture

- **No new syscall.** Arnold can already write `.mael` files via `sys_write_file`; he just needs to know the syntax. v0 Arnold's role context includes a Maelstrom syntax primer (link to Maelstrom spec) so synthesized scripts are valid.
- **Reserved v1 slot:** `sys_run_maelstrom { script_path, args, capability_grants }` is the v1 executor syscall. Arnold's syscall harness is built so adding it is mechanical.
- **Composition with Arnold's jail:** Arnold grants Maelstrom only capabilities the user pre-authorized for the calling session. Arnold's jail + Maelstrom's capability checks compose; nothing escapes either layer.

### User-facing artifact path

`~/.arnold/scripts/*.mael` — Arnold writes here, user reads/edits/runs (manually via `mael run` until v1 executor lands).

This gives users a script library that accumulates from v0 onward (text-only) and becomes Arnold-executable on v1 day — no migration, no rewrites.

---

## Phasing / Milestones

| Milestone | What ships | Blocking on |
|---|---|---|
| v0 | This spec's "in scope" set | Implementation only |
| v0.1 | `sys_web_search`, `sys_schedule`, `sys_cancel_job`, `sys_log`, `sys_delete_file`, `sys_create_dir` | v0 in use; user feedback |
| v1 | Hybrid memory (markdown + vector/FTS), `sys_run_maelstrom`, broader personal-agent syscalls (browser, calendar, mail per killer use cases), containerized Arnold daemon | Maelstrom impl; v0 user signal on which surfaces matter |
| v2 | Cloud Arnold deployment; cloud 439 backend (`ContainerRuntime` impl); web UI; multi-device clients | Infra build; cloud product decisions |

---

## Open Questions (to resolve during implementation planning)

- **`cpu` binary versioning.** When 439's `cpu` evolves (new providers, schema changes), Arnold needs to be compatible. Use the same `vendor/439` pinned tag for both `bios` and `cpu`? Yes — same submodule, same build.
- **Workspace export contract.** BIOS exports the verified workspace to a host path. Confirm during planning that BIOS' export format includes the verdict JSON (success/fail, evidence summary) Arnold needs to construct the `job_completed` event, vs. Arnold having to parse BIOS' stdout.
- **macOS launchd vs `arnoldd start` lazy spawn.** For dev iteration, lazy spawn on first `arnold` invocation might be friendlier than always-on launchd. Decide during planning.
- **TUI ↔ daemon wire schema.** JSON-line is the floor; specific frame types (request/response, server-push for inbox events) get nailed down during planning. Consider matching 439's frame style for consistency.
- **Cost meter source.** Arnold's status bar wants "session cost so far." Reusing 439's `usage` frame from `cpu` is natural; confirm during planning that the daemon can intercept those frames cleanly.
- **Default jail policy.** v0 jail = `~/.arnold/workspace/` + user-allowed dirs from config. Define the config format and the user UX for adding allowed dirs from inside Arnold.

---

## Non-Goals (v0)

- Not a chat playground or general-purpose chatbot.
- Not a replacement for 439 — Arnold *uses* 439.
- Not multi-tenant. v0 has one user (the daemon owner).
- Not a SaaS product. Local-only for v0.
- Not a domain pack for 439 — Arnold is a sibling product that *consumes* packs via 439, not a pack itself.

---

## Appendix: Decision Provenance

This spec is the output of a brainstorm session held 2026-05-20. Key decisions and their rationale:

| Decision | Rationale |
|---|---|
| Two separate products | 439's customer (runtime builders) and Arnold's customer (agent users) are different categories with different shapes; conflating them weakens both |
| Arnold = single agent, not multicore | Persistent personal-agent shape doesn't fit 439's planner/developer/verifier decomposition; reuse syscall *idea* not multicore *machinery* |
| Long-lived daemon, not container, for v0 | Avoids Docker-in-Docker for v0; container is for v1 distribution; persistence comes from `~/.arnold/` either way |
| `sys_spin_up_439` async + inbox model | Required to keep Arnold responsive while builds run; only model that scales cleanly to cloud 439 later |
| Hybrid dependency (subprocess BIOS + library cpu/providers) | BIOS is a stable CLI surface (cheapest coupling for the 439 runtime); cpu/providers represent polished provider work not worth reimplementing |
| Markdown memory v0, hybrid v1 | Mirror of the auto-memory pattern the author uses daily; vector retrieval is a v1 add when corpus outgrows context |
| Go TUI (bubbletea), Rust daemon | Org-level standardization with another 439-adjacent product; UDS process boundary keeps polyglot cost contained |
| Maelstrom = v1 integration, v0 syntax-aware only | Maelstrom is design-phase; v0 accumulates `.mael` artifacts that become executable on v1 day; zero v0 dependency |
| Local-first both Arnold and 439 | Validates architecture without infra build; cloud is a swap of `ContainerRuntime` + deployment target later |
