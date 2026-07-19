//! Module-owned SQLite storage for RA game/hash matches
//! (`docs/retroachievements-design.md` §4, sub-phase 6a scope only: hashing
//! and matching, T1 anonymous). Achievement/unlock tables from §4.2 are
//! created by the same migration (schema is cheaper to ship once, up
//! front) but have no accessors here yet — those land with sub-phases 6b/6c.
//!
//! Versioned independently of the core schema via
//! `relic_core::db::apply_module_migrations`, same pattern
//! `relic-scraper` uses. Dropping every `ra_` table fully de-integrates the
//! module (design doc §4.3) — no core table references them.
//!
//! Not yet implemented: the "no-match" TTL cache design doc §2.3 mentions
//! (so a hash confirmed absent from RA's catalog isn't re-queried every
//! launch). Left out of this schema deliberately — the design doc's own
//! open question #8 (synthetic hash fixtures for testing) isn't resolved
//! yet, and speculating on that shape now risks a migration that has to be
//! reworked before it ships any real data.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_ra_tables.sql"),
    include_str!("../migrations/0002_ra_no_match_cache.sql"),
];

/// Bring the module's own tables up to date. Call once per connection
/// before using any other function in this module.
pub fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    relic_core::db::apply_module_migrations(conn, "retroachievements", MIGRATIONS)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaGameMatch {
    pub ra_game_id: i64,
    pub console_id: Option<i64>,
    pub title: Option<String>,
    pub hash: String,
    pub relic_file_id: i64,
}

/// Record a confirmed hash → RA game id match for `relic_file_id`. Called
/// after a successful hash-library lookup (design doc §2.3); overwrites any
/// prior match for the same `(hash, relic_file_id)` pair.
pub fn save_match(conn: &Connection, m: &RaGameMatch) -> rusqlite::Result<()> {
    let matched_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO ra_games (ra_game_id, console_id, title, hash, relic_file_id, matched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(hash, relic_file_id) DO UPDATE SET
             ra_game_id = excluded.ra_game_id,
             console_id = excluded.console_id,
             title = excluded.title,
             matched_at = excluded.matched_at",
        params![
            m.ra_game_id,
            m.console_id,
            m.title,
            m.hash,
            m.relic_file_id,
            matched_at,
        ],
    )?;
    Ok(())
}

/// Every recorded match for a file (normally zero or one; a file can have
/// more than one candidate hash for multi-hash consoles, design doc §2.3).
pub fn matches_for_file(
    conn: &Connection,
    relic_file_id: i64,
) -> rusqlite::Result<Vec<RaGameMatch>> {
    let mut stmt = conn.prepare(
        "SELECT ra_game_id, console_id, title, hash, relic_file_id
         FROM ra_games WHERE relic_file_id = ?1",
    )?;
    let rows = stmt.query_map(params![relic_file_id], |row| {
        Ok(RaGameMatch {
            ra_game_id: row.get(0)?,
            console_id: row.get(1)?,
            title: row.get(2)?,
            hash: row.get(3)?,
            relic_file_id: row.get(4)?,
        })
    })?;
    rows.collect()
}

/// "Has cheevos" smart-collection predicate (design doc §5): true iff any
/// match is recorded for this file.
pub fn has_cheevos(conn: &Connection, relic_file_id: i64) -> rusqlite::Result<bool> {
    Ok(!matches_for_file(conn, relic_file_id)?.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_match(relic_file_id: i64) -> RaGameMatch {
        RaGameMatch {
            ra_game_id: 42,
            console_id: Some(3),
            title: Some("Super Metroid".to_string()),
            hash: "deadbeefcafef00d".to_string(),
            relic_file_id,
        }
    }

    #[test]
    fn no_match_means_no_cheevos() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        assert!(!has_cheevos(&conn, 1).unwrap());
        assert!(matches_for_file(&conn, 1).unwrap().is_empty());
    }

    #[test]
    fn saved_match_is_found_by_file_id() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        save_match(&conn, &sample_match(7)).unwrap();

        assert!(has_cheevos(&conn, 7).unwrap());
        let found = matches_for_file(&conn, 7).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].ra_game_id, 42);
        assert_eq!(found[0].title.as_deref(), Some("Super Metroid"));

        // A different file is unaffected.
        assert!(!has_cheevos(&conn, 8).unwrap());
    }

    #[test]
    fn saving_again_overwrites_the_same_hash_file_pair() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        save_match(&conn, &sample_match(7)).unwrap();

        let mut updated = sample_match(7);
        updated.title = Some("Super Metroid (Updated Title)".to_string());
        save_match(&conn, &updated).unwrap();

        let found = matches_for_file(&conn, 7).unwrap();
        assert_eq!(
            found.len(),
            1,
            "same (hash, relic_file_id) must overwrite, not duplicate"
        );
        assert_eq!(
            found[0].title.as_deref(),
            Some("Super Metroid (Updated Title)")
        );
    }
}
