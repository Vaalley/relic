//! relic-themes — the theme engine (PLAN.md §6).
//!
//! Two layers, shipped in order:
//! - Layer 1 (1.0): design tokens — colors, typography, shape, sounds —
//!   loaded from a theme manifest (`theme.toml` + assets); see
//!   `themes/default/theme.toml` for the shape of a token theme.
//! - Layer 2 (post-1.0): declarative, data-bound screen layouts.
//!
//! Themes are pure data + assets: no network access and no filesystem
//! access outside their own folder. A broken theme must degrade to the
//! default theme with a visible warning, never a crash.
//!
//! Planned for Phase 5, isolated in its own crate so a broken theme can
//! never take down the core engine.

/// Stable identifier this module reports through the engine's
/// `capabilities()` API so shells can hide UI for absent modules.
pub fn capability_id() -> &'static str {
    "themes"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_id_is_themes() {
        assert_eq!(capability_id(), "themes");
    }
}
