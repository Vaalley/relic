# .agents — multi-agent workflow

This repo is worked on by multiple coding agents in parallel. This folder holds
the shared plumbing so the workflow is reproducible on any machine.

## Instruction files

- `AGENTS.md` (repo root) — canonical instructions, the cross-vendor standard.
  Read natively by Devin, Antigravity, Codex, and Gemini.
- `CLAUDE.md` — thin Claude Code entry point that `@AGENTS.md`-imports the
  canonical file. (A true symlink needs Windows Developer Mode enabled; the
  `@import` is equivalent and survives git + editors on every platform.)

## Installed agent CLIs (verified 2026-07-18)

| Agent | Binary | Headless one-shot | Notes |
|---|---|---|---|
| Claude Code | `claude` | `claude -p "…"` | Orchestrator; spawns its own subagents (Sonnet for boilerplate). |
| Devin | `devin` | `devin -p --permission-mode accept-edits -- "…"` | Modes: auto / accept-edits / smart / dangerous. |
| Antigravity | `agy` | `agy -p "…" --mode accept-edits --print-timeout 15m` | `agy agents`/`agy models` can hang; `-p` works fine. |
| Codex | `codex` | available | Not wired into dispatch yet. |
| Gemini | `gemini` | available | Not wired into dispatch yet. |

## Dispatching work

`dispatch.ps1` fans a task out to an agent, logging to `.agents/runs/` (gitignored):

```powershell
# One-shot, wait for result:
.\.agents\dispatch.ps1 -Agent devin -Task "Draft docs/foo.md per PLAN.md §6"

# Fire-and-forget (parallel fan-out — run several, then check logs):
.\.agents\dispatch.ps1 -Agent agy -Task "..." -Background
.\.agents\dispatch.ps1 -Agent devin -Task "..." -Background
```

## Ground rules for fan-out (the orchestrator enforces these)

1. **Non-overlapping file sets.** Parallel tasks must name disjoint paths; the
   task brief says which files the agent may create/edit.
2. **Edits-only permission level.** Dispatched agents get accept-edits, not
   arbitrary command execution; the orchestrator runs build/test/lint afterwards
   and owns the verdict. Never pass a dangerous/skip-permissions flag from dispatch.
3. **Verification is not delegated.** Nothing an agent reports is "done" until
   `cargo fmt --check && cargo clippy -- -D warnings && cargo test --workspace`
   passes in the orchestrating session.
4. Task briefs are self-contained: point at PLAN.md sections, name exact files,
   state the acceptance check. Agents don't inherit chat context.
