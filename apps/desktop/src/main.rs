//! Relic desktop shell (stub).
//!
//! This binary is a placeholder pending Phase 2 implementation. The UI stack
//! decision (ADR-002, `docs/adr/0002-desktop-ui-stack.md`) is settled: Slint.
//! This becomes the real system browser / grid / detail / launch shell
//! (PLAN.md Phase 2) once that work starts.

use relic_core::api::Engine;

fn main() {
    let engine = Engine::open_in_memory().expect("in-memory engine should always open");
    let systems = engine.list_systems().unwrap_or_default();
    println!(
        "Relic desktop shell (stub) — core {}, {} systems registered",
        engine.version(),
        systems.len()
    );
}
