//! Relic desktop shell (stub).
//!
//! This binary is a placeholder pending the Phase 0 desktop UI spike: Slint
//! vs egui is an open decision (ADR-002, `docs/adr/0002-desktop-ui-stack.md`),
//! decided by building a gamepad-navigable 1000-item grid at 60 fps in each
//! and picking the winner with evidence. Once that lands, this becomes the
//! real system browser / grid / detail / launch shell (PLAN.md Phase 2).

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
