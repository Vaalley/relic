//! relic-scraper — opt-in metadata and media scraping (PLAN.md §7.1).
//!
//! T1: no network activity unless the user enables this module and supplies
//! their own provider credentials where required. Providers (ScreenScraper,
//! TheGamesDB, LaunchBox mirror) sit behind one trait (`provider`) so the
//! matching and caching pipeline stays provider-agnostic. Matching
//! (`pipeline`) is hash-first (DAT/RA hash), falling back to filename
//! heuristics with a confirmation UI for low-confidence matches. Matches are
//! persisted in module-owned tables (`store`), versioned independently of
//! the core schema — see `docs/retroachievements-design.md` §4 for the
//! shared module-migration pattern. Fetched media flows into the same
//! content-addressed cache as local media (not yet wired — no concrete
//! provider exists yet).
//!
//! Planned for Phase 4, alongside DAT matching and gamelist export.

pub mod pipeline;
pub mod provider;
pub mod providers;
pub mod store;

pub use pipeline::{match_game, Confidence, MatchResult, AUTO_APPLY_THRESHOLD};
pub use provider::{Candidate, Provider, ProviderError, SearchQuery};
pub use store::{confirm_match, migrate, pending_matches, save_match, StoredMatch};

/// Stable identifier this module reports through the engine's
/// `capabilities()` API so shells can hide UI for absent modules.
pub fn capability_id() -> &'static str {
    "scraper"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_id_is_scraper() {
        assert_eq!(capability_id(), "scraper");
    }
}
