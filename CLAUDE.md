# Arnold — Operating Manual for AI Agents

Read first:
1. README.md
2. docs/superpowers/specs/2026-05-20-arnold-personal-agent-design.md (the spec)
3. docs/superpowers/plans/ (active plan)

This repo follows the same naming and contribution discipline as the [439 codebase](https://github.com/jbankse/439): conventional commits, per-crate cargo discipline (`cargo check -p <crate>`, not workspace-wide), snake_case files, branch-per-change.

The `vendor/439` submodule is a black-box dependency; do not edit files inside it. Bump the pinned ref with `git submodule update --remote vendor/439` when you intentionally pull in a new 439 version, and capture the bump in a `chore(vendor): bump 439 to <sha>` commit.

## Toolchain note (v0b+)

The repo now contains two Go modules (`arnold-tui/` and `arnold-tray/`) alongside the Cargo workspace (`arnold-wire`, `arnoldd`). The Go modules are siblings — they don't share Go workspace state and don't link against the Cargo workspace. Build them with `go build ./arnold-tui/...` and `go build ./arnold-tray/...` respectively; their install paths are handled by `scripts/build-tui.sh` / `scripts/build-tray.sh`.

When changing wire types, edit `arnold-wire/src/lib.rs` AND `arnold-tui/wire/wire.go` together — the Go file is a hand-maintained mirror, and the JSON shape must stay byte-identical between the two. A common bug pattern is to add a Rust variant without updating the Go struct; the daemon will emit it, the TUI will silently ignore it.
