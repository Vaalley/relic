//! Hash-first, filename-fallback matching pipeline (PLAN.md §7.1): a game is
//! matched by content hash first (`Confidence::Exact`); only when no
//! provider returns a hash hit does filename similarity kick in, with
//! confidence degrading accordingly. Anything below `AUTO_APPLY_THRESHOLD`
//! must go through a confirmation UI rather than being applied silently.

use std::path::Path;

use crate::provider::{Candidate, Provider, ProviderError, SearchQuery};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    Low,
    Medium,
    High,
    /// Content-hash match — the same file, byte for byte, as far as the
    /// provider's index is concerned.
    Exact,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Low => "low",
            Confidence::Medium => "medium",
            Confidence::High => "high",
            Confidence::Exact => "exact",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Confidence::Low),
            "medium" => Some(Confidence::Medium),
            "high" => Some(Confidence::High),
            "exact" => Some(Confidence::Exact),
            _ => None,
        }
    }
}

/// Matches at or above this confidence may be applied without asking the
/// user first; anything below needs the match-confirmation UI (PLAN.md §7.1).
pub const AUTO_APPLY_THRESHOLD: Confidence = Confidence::High;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchResult {
    pub provider_id: &'static str,
    pub candidate: Candidate,
    pub confidence: Confidence,
}

/// Run the matching pipeline for one file against every configured
/// provider, hash-first: if any provider returns a hash hit, filename
/// search is skipped entirely (a hash hit is authoritative). Results are
/// sorted highest-confidence first.
pub fn match_game(
    providers: &[&dyn Provider],
    query: &SearchQuery,
) -> Result<Vec<MatchResult>, ProviderError> {
    let mut results = Vec::new();
    for provider in providers {
        for candidate in provider.search_by_hash(query)? {
            results.push(MatchResult {
                provider_id: provider.id(),
                candidate,
                confidence: Confidence::Exact,
            });
        }
    }
    if !results.is_empty() {
        return Ok(results);
    }

    for provider in providers {
        for candidate in provider.search_by_name(query)? {
            let confidence = score_filename(query.filename, &candidate.name);
            results.push(MatchResult {
                provider_id: provider.id(),
                candidate,
                confidence,
            });
        }
    }
    results.sort_by_key(|r| std::cmp::Reverse(r.confidence));
    Ok(results)
}

/// Crude filename similarity: strip the extension and any non-alphanumeric
/// noise from both sides, then compare. Good enough to separate "obviously
/// the same title" from "share a substring" from "unrelated"; a provider
/// wanting real fuzzy ranking (edit distance, region tag stripping) can
/// still return its own ordering — this is just the pipeline's fallback
/// scorer when candidates are otherwise unranked.
fn score_filename(filename: &str, candidate_name: &str) -> Confidence {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);
    let a = normalize(stem);
    let b = normalize(candidate_name);
    if a.is_empty() || b.is_empty() {
        return Confidence::Low;
    }
    if a == b {
        Confidence::High
    } else if a.contains(&b) || b.contains(&a) {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        id: &'static str,
        hash_hits: Vec<Candidate>,
        name_hits: Vec<Candidate>,
    }

    impl Provider for MockProvider {
        fn id(&self) -> &'static str {
            self.id
        }

        fn search_by_hash(&self, _query: &SearchQuery) -> Result<Vec<Candidate>, ProviderError> {
            Ok(self.hash_hits.clone())
        }

        fn search_by_name(&self, _query: &SearchQuery) -> Result<Vec<Candidate>, ProviderError> {
            Ok(self.name_hits.clone())
        }
    }

    fn candidate(name: &str) -> Candidate {
        Candidate {
            external_id: "1".into(),
            name: name.into(),
            system_slug: "snes".into(),
        }
    }

    #[test]
    fn hash_hit_short_circuits_filename_search() {
        let provider = MockProvider {
            id: "mock",
            hash_hits: vec![candidate("Super Metroid")],
            name_hits: vec![candidate("Wrong Game")],
        };
        let query = SearchQuery {
            system_slug: "snes",
            filename: "Super Metroid (USA).sfc",
            crc32: Some(0xDEADBEEF),
            md5: None,
        };
        let results = match_game(&[&provider], &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].confidence, Confidence::Exact);
        assert_eq!(results[0].candidate.name, "Super Metroid");
    }

    #[test]
    fn exact_filename_match_scores_high_not_exact() {
        let provider = MockProvider {
            id: "mock",
            hash_hits: vec![],
            name_hits: vec![candidate("super metroid")],
        };
        let query = SearchQuery {
            system_slug: "snes",
            filename: "Super Metroid.sfc",
            crc32: None,
            md5: None,
        };
        let results = match_game(&[&provider], &query).unwrap();
        assert_eq!(results[0].confidence, Confidence::High);
    }

    #[test]
    fn unrelated_filename_scores_low() {
        let provider = MockProvider {
            id: "mock",
            hash_hits: vec![],
            name_hits: vec![candidate("Completely Different Title")],
        };
        let query = SearchQuery {
            system_slug: "snes",
            filename: "Super Metroid.sfc",
            crc32: None,
            md5: None,
        };
        let results = match_game(&[&provider], &query).unwrap();
        assert_eq!(results[0].confidence, Confidence::Low);
        assert!(Confidence::Low < AUTO_APPLY_THRESHOLD);
    }

    #[test]
    fn results_sorted_highest_confidence_first() {
        let provider = MockProvider {
            id: "mock",
            hash_hits: vec![],
            name_hits: vec![candidate("Totally Unrelated"), candidate("Super Metroid")],
        };
        let query = SearchQuery {
            system_slug: "snes",
            filename: "Super Metroid.sfc",
            crc32: None,
            md5: None,
        };
        let results = match_game(&[&provider], &query).unwrap();
        assert_eq!(results[0].candidate.name, "Super Metroid");
        assert_eq!(results[0].confidence, Confidence::High);
    }
}
