# Arnold

A long-running, code-focused personal agent that lives as a persistent daemon. Spawns ephemeral [439](https://github.com/jbankse/439) environments as a native syscall capability while staying responsive to ongoing conversation.

Design spec: [docs/superpowers/specs/2026-05-20-arnold-personal-agent-design.md](docs/superpowers/specs/2026-05-20-arnold-personal-agent-design.md)

v0a implementation plan: [docs/superpowers/plans/2026-05-20-arnold-v0a-standalone-assistant.md](docs/superpowers/plans/2026-05-20-arnold-v0a-standalone-assistant.md)

v0b implementation plan: [docs/superpowers/plans/2026-05-21-arnold-v0b-439-dispatch-and-tui.md](docs/superpowers/plans/2026-05-21-arnold-v0b-439-dispatch-and-tui.md)

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
- The directory you launch `arnold` from is auto-allowlisted for that session — no manual `config.toml` editing to start working in a new project

For a pre-flight health check without spending an LLM turn:

```bash
arnoldd --check
```
