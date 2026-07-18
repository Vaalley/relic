---
name: dispatch
description: Fan well-specified subtasks out to the locally installed Devin and Antigravity CLIs in parallel, alongside Claude subagents. Use when the user asks to parallelize work across their other AI agents, or when independent well-specified tasks can run concurrently.
---

# Multi-agent dispatch

This project fans independent tasks out to other installed agent CLIs. Full
workflow and CLI reference: `.agents/README.md` (read it if anything below fails).

## How to dispatch

Run via PowerShell tool, one call per agent, in parallel tool calls when independent.
Foreground (≤10 min tasks) so completion is observed; use `-Background` +
`run_in_background` for longer work:

```powershell
.\.agents\dispatch.ps1 -Agent devin -Task "<self-contained brief>"
.\.agents\dispatch.ps1 -Agent agy   -Task "<self-contained brief>"
```

Logs land in `.agents/runs/` (gitignored).

## Rules (non-negotiable)

1. Parallel tasks get **disjoint file sets**; the brief names exactly which files
   the agent may create/edit, and says "do not commit".
2. Briefs are self-contained: cite PLAN.md sections, exact paths, acceptance
   criteria. Dispatched agents have no chat context.
3. Never raise permissions beyond accept-edits from a dispatch (no
   `--dangerously-skip-permissions`, no devin `dangerous` mode).
4. After completion, **verify in this session**:
   `cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace`
   and review the diff of the files the brief allowed. Fix or re-dispatch as needed.

## Choosing an executor

- Claude subagent (Agent tool, `model: sonnet`) — default for boilerplate; inherits
  nothing but follows specs well and can run cargo itself.
- Devin (`devin`) — good for self-contained docs/specs and multi-file mechanical edits.
- Antigravity (`agy`) — good for data-file batches and small scoped code edits.
- Keep design decisions and core-engine logic in the main session.
