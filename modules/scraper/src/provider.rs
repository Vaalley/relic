//! The `Provider` trait: one interface for every metadata/media source
//! (ScreenScraper, TheGamesDB, LaunchBox mirror — PLAN.md §7.1), so matching
//! and caching stay provider-agnostic. Concrete HTTP clients live behind
//! this trait in a `providers/` submodule (not yet added).

use std::fmt;

/// A candidate match returned by a provider, before confidence scoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Provider-native identifier for this game (opaque outside the provider).
    pub external_id: String,
    pub name: String,
    pub system_slug: String,
}

/// What we know about a scanned file, handed to a provider to search with.
#[derive(Debug, Clone, Copy)]
pub struct SearchQuery<'a> {
    pub system_slug: &'a str,
    pub filename: &'a str,
    pub crc32: Option<u32>,
    pub md5: Option<&'a str>,
}

#[derive(Debug)]
pub enum ProviderError {
    Network(String),
    RateLimited,
    /// The provider needs user-supplied credentials that are missing.
    NotConfigured,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Network(msg) => write!(f, "network error: {msg}"),
            ProviderError::RateLimited => write!(f, "rate limited"),
            ProviderError::NotConfigured => write!(f, "provider not configured"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// A metadata/media source. Hash lookup is exact; providers that can't
/// support it (no hash index) return `Ok(vec![])` rather than erroring, so
/// the pipeline can fall through to filename search uniformly.
pub trait Provider {
    /// Stable identifier, e.g. `"screenscraper"`. Stored alongside matches.
    fn id(&self) -> &'static str;

    fn search_by_hash(&self, query: &SearchQuery) -> Result<Vec<Candidate>, ProviderError>;

    fn search_by_name(&self, query: &SearchQuery) -> Result<Vec<Candidate>, ProviderError>;
}
