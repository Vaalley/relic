# ADR 0002: Desktop UI stack — Slint vs egui

## Status

Accepted — Slint.

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

PLAN.md §9 originally called for a one-week spike building the same
gamepad-navigable grid in both stacks and measuring frame times. That spike
was not run — this environment has no GPU/display surface to drive or
observe a 60fps interactive session on. The decision below was made instead
by license analysis and architectural fit against Relic's actual
requirements, at the owner's explicit direction to decide rather than block
further on the spike. If real-world use surfaces a Slint dealbreaker (see
Consequences), this ADR should be reopened rather than silently overridden.

## Decision

**Slint.** Two independent lines of reasoning converged:

1. **Licensing is a non-issue for Relic specifically.** Slint is
   triple-licensed (royalty-free-with-attribution, commercial, or GPLv3).
   The royalty-free tier excludes embedded targets and requires an
   attribution notice — friction for most projects. Neither applies here:
   `Cargo.toml` already sets `license = "GPL-3.0-or-later"` for the whole
   workspace, so Relic qualifies for Slint's GPLv3 tier outright — free,
   no attribution requirement, no royalty, no commercial-tier negotiation
   ever needed. This removes what is normally Slint's biggest adoption
   risk. egui (MIT/Apache-2.0) has no licensing edge here since Relic's
   own license is already copyleft.
2. **Architectural fit favors Slint for this specific UI.** The desktop
   shell's defining requirements are (a) a declarative, themeable 10-foot
   grid and (b) Phase 5's design-token theme engine (`modules/themes`)
   driving it directly. Slint's `.slint` DSL is built around exactly that:
   declarative components with a token-mappable style system. egui is
   immediate-mode — simpler to prototype, but styling is applied
   imperatively per-frame via `egui::Style`/`Visuals`, a weaker fit for a
   community-authored, hot-reloadable theme format (PLAN.md §6). There is
   also a working precedent for the gamepad-driven case specifically:
   [gpcl](https://github.com/dngulin/gpcl), a gamepad-controlled launcher
   built in Rust + Slint over winit.

Sources consulted: [Slint LICENSE.md](https://github.com/slint-ui/slint/blob/master/LICENSE.md),
[Slint pricing/FAQ](https://slint.dev/pricing), [egui repo](https://github.com/emilk/egui).

The unresolved question this ADR does *not* answer is runtime performance
on real target hardware (2015-era laptop, 1000-item grid, 60fps, no
GC/alloc hitches) — that remains an open risk to watch during Phase 2
implementation, not a spike result to cite.

## Consequences

- `apps/desktop` moves from a stub to real Phase 2 implementation using
  Slint (`slint` + `slint-build` crates), starting with a minimal window
  that proves the build/packaging pipeline before the full grid/browser/
  detail UI.
- Packaging targets MSI (Windows), dmg (macOS), flatpak/AppImage (Linux) —
  Slint has first-class support for all four via its own packaging guides.
- The GPLv3 licensing choice for Relic is now load-bearing for the desktop
  shell's dependency choice, not just a preference; changing Relic's
  license later would reopen the Slint-vs-egui royalty question.
- If the 60fps/no-hitches exit criterion turns out not to hold on real
  hardware once Phase 2 is underway, that is grounds to reopen this ADR —
  the decision above is architectural, not an empirical performance
  guarantee.
