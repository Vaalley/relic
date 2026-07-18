//! relic-sync — LAN save & user-data sync between the user's own devices
//! (PLAN.md §4.5, §7).
//!
//! T0/T1: discovery and transfer happen only between devices the user owns
//! and pairs directly on their own network; no relay, no accounts. Syncs
//! `user_data`, `collections`, `play_sessions`, and save files, never
//! scanned/rebuildable library data. This is deliberately separate from the
//! later, smaller-scoped "Relic Circle" friends feature (§7.3).
//!
//! Planned for Phase 7, sized after the sync-vs-friends split is evaluated.

/// Stable identifier this module reports through the engine's
/// `capabilities()` API so shells can hide UI for absent modules.
pub fn capability_id() -> &'static str {
    "sync"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_id_is_sync() {
        assert_eq!(capability_id(), "sync");
    }
}
