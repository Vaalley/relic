//! SQLite-backed match cache and confirmation state. Module-owned
//! (`docs/retroachievements-design.md` §4 documents the same pattern for the
//! RA module); lives in `scraper_matches`, versioned independently of the
//! core schema via `relic_core::db::apply_module_migrations`. Dropping this
//! table fully de-integrates the module — no core table references it.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::pipeline::{Confidence, MatchResult};

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_scraper_matches.sql")];

/// Bring the module's own tables up to date. Call once per connection
/// before using any other function in this module — same discipline as
/// `relic_core::db::Db::open` for the core schema.
pub fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    relic_core::db::apply_module_migrations(conn, "scraper", MIGRATIONS)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredMatch {
    pub game_id: i64,
    pub provider_id: String,
    pub external_id: String,
    pub confidence: Confidence,
    pub confirmed: bool,
}

/// Record (or overwrite) a match for `game_id` from one provider. Confidence
/// at or above `AUTO_APPLY_THRESHOLD` is stored pre-confirmed; anything
/// lower waits for `confirm_match` (PLAN.md §7.1 "confirmation UI for
/// low-confidence matches").
pub fn save_match(conn: &Connection, game_id: i64, result: &MatchResult) -> rusqlite::Result<()> {
    let confirmed = result.confidence >= crate::pipeline::AUTO_APPLY_THRESHOLD;
    let matched_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO scraper_matches (game_id, provider_id, external_id, confidence, confirmed, matched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(game_id, provider_id) DO UPDATE SET
             external_id = excluded.external_id,
             confidence = excluded.confidence,
             confirmed = excluded.confirmed,
             matched_at = excluded.matched_at",
        params![
            game_id,
            result.provider_id,
            result.candidate.external_id,
            result.confidence.as_str(),
            confirmed as i64,
            matched_at,
        ],
    )?;
    Ok(())
}

/// User confirms a previously low-confidence match — the shell's
/// match-confirmation UI calls this after the user picks a candidate.
pub fn confirm_match(conn: &Connection, game_id: i64, provider_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE scraper_matches SET confirmed = 1 WHERE game_id = ?1 AND provider_id = ?2",
        params![game_id, provider_id],
    )?;
    Ok(())
}

/// Matches still awaiting user confirmation — what the confirmation UI lists.
pub fn pending_matches(conn: &Connection) -> rusqlite::Result<Vec<StoredMatch>> {
    let mut stmt = conn.prepare(
        "SELECT game_id, provider_id, external_id, confidence, confirmed
         FROM scraper_matches WHERE confirmed = 0 ORDER BY game_id",
    )?;
    let rows = stmt.query_map([], |row| {
        let confidence_str: String = row.get(3)?;
        Ok(StoredMatch {
            game_id: row.get(0)?,
            provider_id: row.get(1)?,
            external_id: row.get(2)?,
            confidence: Confidence::parse(&confidence_str).unwrap_or(Confidence::Low),
            confirmed: row.get::<_, i64>(4)? != 0,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Candidate;

    fn result(confidence: Confidence) -> MatchResult {
        MatchResult {
            provider_id: "mock",
            candidate: Candidate {
                external_id: "42".into(),
                name: "Super Metroid".into(),
                system_slug: "snes".into(),
            },
            confidence,
        }
    }

    #[test]
    fn high_confidence_match_is_stored_pre_confirmed() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        save_match(&conn, 1, &result(Confidence::Exact)).unwrap();
        assert!(pending_matches(&conn).unwrap().is_empty());
    }

    #[test]
    fn low_confidence_match_waits_for_confirmation() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        save_match(&conn, 1, &result(Confidence::Low)).unwrap();

        let pending = pending_matches(&conn).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].game_id, 1);
        assert!(!pending[0].confirmed);

        confirm_match(&conn, 1, "mock").unwrap();
        assert!(pending_matches(&conn).unwrap().is_empty());
    }

    #[test]
    fn saving_again_overwrites_the_same_provider_slot() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        save_match(&conn, 1, &result(Confidence::Low)).unwrap();
        save_match(&conn, 1, &result(Confidence::Exact)).unwrap();
        assert!(pending_matches(&conn).unwrap().is_empty());
    }
}
