# Arnold

A long-running, code-focused personal agent that lives as a persistent daemon. Spawns ephemeral [439](https://github.com/jbankse/439) environments as a native syscall capability while staying responsive to ongoing conversation.

Design spec: [docs/superpowers/specs/2026-05-20-arnold-personal-agent-design.md](docs/superpowers/specs/2026-05-20-arnold-personal-agent-design.md)

v0a implementation plan: [docs/superpowers/plans/2026-05-20-arnold-v0a-standalone-assistant.md](docs/superpowers/plans/2026-05-20-arnold-v0a-standalone-assistant.md)

## Quickstart (v0a)

```bash
./scripts/install.sh
export ANTHROPIC_API_KEY=sk-...
~/.local/bin/arnoldd &
~/.local/bin/arnold
> Hello Arnold, what can you do?
```
