//! Concrete `Provider` implementations, one file per provider (PLAN.md §7.1).
//! Each is behind its own cargo feature so a default build of relic-scraper
//! pulls in no HTTP client and makes no network calls.

#[cfg(feature = "screenscraper")]
pub mod screenscraper;
