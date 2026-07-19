//! Smart-collection predicate encoding (PLAN.md Phase 4 "smart collections").
//!
//! A smart collection's filter is stored as plain `key=value` lines in
//! `collections.smart_query` rather than a real query language — enough to
//! express "favorites in this system matching this search text" without
//! inventing (and needing to fuzz-test) a parser. A value may not contain a
//! newline; that's the only restriction, and unknown/malformed lines are
//! dropped rather than rejected, matching this crate's general tolerance
//! rule for stored-but-editable text (see gamelist.rs).
//!
//! `Engine` (core/src/api/mod.rs) owns the SQL that evaluates a `SmartQuery`
//! against `games`/`user_data` — this module only encodes/decodes the
//! predicate itself, so it stays free of the `GameRow`/`Db` types and can't
//! create a dependency cycle with `api`.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmartQuery {
    pub system: Option<String>,
    pub search: Option<String>,
    pub favorite: Option<bool>,
}

impl SmartQuery {
    pub fn encode(&self) -> String {
        let mut lines = Vec::new();
        if let Some(v) = &self.system {
            lines.push(format!("system={v}"));
        }
        if let Some(v) = &self.search {
            lines.push(format!("search={v}"));
        }
        if let Some(v) = self.favorite {
            lines.push(format!("favorite={v}"));
        }
        lines.join("\n")
    }

    pub fn parse(s: &str) -> Self {
        let mut q = SmartQuery::default();
        for line in s.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "system" => q.system = Some(value.to_string()),
                "search" => q.search = Some(value.to_string()),
                "favorite" => q.favorite = value.parse::<bool>().ok(),
                _ => {}
            }
        }
        q
    }

    pub fn is_empty(&self) -> bool {
        self.system.is_none() && self.search.is_none() && self.favorite.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_all_fields() {
        let q = SmartQuery {
            system: Some("snes".into()),
            search: Some("mario".into()),
            favorite: Some(true),
        };
        assert_eq!(SmartQuery::parse(&q.encode()), q);
    }

    #[test]
    fn empty_query_round_trips() {
        let q = SmartQuery::default();
        assert!(SmartQuery::parse(&q.encode()).is_empty());
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let q = SmartQuery::parse("bogus=1\nsystem=nes");
        assert_eq!(q.system.as_deref(), Some("nes"));
        assert_eq!(q.search, None);
    }

    #[test]
    fn malformed_favorite_value_is_dropped() {
        let q = SmartQuery::parse("favorite=maybe");
        assert_eq!(q.favorite, None);
    }

    #[test]
    fn value_containing_equals_sign_keeps_only_first_split() {
        let q = SmartQuery::parse("search=mario=luigi");
        assert_eq!(q.search.as_deref(), Some("mario=luigi"));
    }
}
