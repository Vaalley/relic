//! relic-scraper — opt-in metadata and media scraping (PLAN.md §7.1).
//!
//! T1: no network activity unless the user enables this module and supplies
//! their own provider credentials where required. Providers (ScreenScraper,
//! TheGamesDB, LaunchBox mirror) sit behind one trait so the matching and
//! caching pipeline stays provider-agnostic. Matching is hash-first (DAT/RA
//! hash), falling back to filename heuristics with a confirmation UI for
//! low-confidence matches. Fetched media flows into the same content-
//! addressed cache as local media.
//!
//! Planned for Phase 4, alongside DAT matching and gamelist export.

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
