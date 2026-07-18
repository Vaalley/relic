# ADR 0002: Desktop UI stack — Slint vs egui

## Status

Proposed

## Context

The desktop shell (`apps/desktop`) needs a Rust-native UI stack for a
gamepad-driven, 10-foot, theme-able game grid across Windows, macOS, and
Linux, without the footprint or focus-handling drawbacks of a webview
(Tauri/Electron — see PLAN.md §2.2 for why those are ruled out).

Two candidates, per PLAN.md §2.2 and §9 (Phase 0):

- **Slint** (primary candidate) — GPU-accelerated, declarative UI language
  that maps well to design-token theming; tiny runtime, no bundled webview.
- **egui** (fallback candidate) — faster to prototype, immediate-mode,
  weaker out-of-the-box styling/theming story.

This is explicitly a Phase 0 decision, made by evidence rather than
preference: PLAN.md §9 calls for a one-week spike building the same
gamepad-navigable grid in both.

## Decision

Not yet decided. Resolve by building a **gamepad-navigable, themed 1000-item
grid** in both Slint and egui and measuring against one exit criterion:

> **60 fps, no GC/alloc hitches, on a 2015-era laptop**, gamepad-navigable,
> with the design-token theme model (PLAN.md §6 layer 1) applied.

Record the spike results in this ADR (frame times, memory, theming
ergonomics, packaging size per shell) and update Status to Accepted once one
stack is chosen. This blocks Phase 2 (desktop shell MVP) per PLAN.md §9.

## Consequences

Until this ADR is accepted, `apps/desktop` stays a stub (see its
`src/main.rs` doc comment) rather than committing to a widget toolkit that
might be thrown away. Whichever stack wins determines the desktop shell's
dependency footprint, packaging story (MSI/dmg/flatpak/AppImage), and how
directly the theme engine (`modules/themes`) can drive its rendering.
