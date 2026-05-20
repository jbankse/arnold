# Arnold — Operating Manual for AI Agents

Read first:
1. README.md
2. docs/superpowers/specs/2026-05-20-arnold-personal-agent-design.md (the spec)
3. docs/superpowers/plans/ (active plan)

This repo follows the same naming and contribution discipline as the [439 codebase](https://github.com/jbankse/439): conventional commits, per-crate cargo discipline (`cargo check -p <crate>`, not workspace-wide), snake_case files, branch-per-change.

The `vendor/439` submodule is a black-box dependency; do not edit files inside it. Bump the pinned ref with `git submodule update --remote vendor/439` when you intentionally pull in a new 439 version, and capture the bump in a `chore(vendor): bump 439 to <sha>` commit.
