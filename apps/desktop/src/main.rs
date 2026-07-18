//! Relic desktop shell (stub).
//!
//! This binary is a placeholder pending Phase 2 implementation. The UI stack
//! decision (ADR-002, `docs/adr/0002-desktop-ui-stack.md`) is settled: Slint.
//! `ui/app.slint` is a toolchain proof-of-concept window, not the real UI —
//! the system browser / grid / detail / launch shell (PLAN.md Phase 2) is
//! built out from here.

slint::include_modules!();

use relic_core::api::Engine;

fn main() -> Result<(), slint::PlatformError> {
    let engine = Engine::open_in_memory().expect("in-memory engine should always open");
    let systems = engine.list_systems().unwrap_or_default();

    let window = MainWindow::new()?;
    window.set_status_line(
        format!(
            "core {} — {} systems registered",
            engine.version(),
            systems.len()
        )
        .into(),
    );
    window.run()
}
