//! No-match TTL cache implementation (design doc §2.3).
//!
//! Stores and queries hashes that are known to not exist in the RetroAchievements database,
//! avoiding repeated queries to the API on launch.

use rusqlite::{params, Connection, OptionalExtension};
use std::time::{SystemTime, UNIX_EPOCH};

/// Checks if a hash is currently cached as "no-match" within the given TTL window.
pub fn is_cached_no_match(conn: &Connection, hash: &str, ttl_secs: u64) -> rusqlite::Result<bool> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let checked_at: Option<i64> = conn
        .query_row(
            "SELECT checked_at FROM ra_no_match WHERE hash = ?1",
            params![hash],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(checked_at) = checked_at {
        let age_secs = now.saturating_sub(checked_at);
        Ok(age_secs >= 0 && (age_secs as u64) < ttl_secs)
    } else {
        Ok(false)
    }
}

/// Records a hash as having "no-match" in the cache, with the current timestamp.
pub fn record_no_match(conn: &Connection, hash: &str) -> rusqlite::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO ra_no_match (hash, checked_at) VALUES (?1, ?2)
         ON CONFLICT(hash) DO UPDATE SET checked_at = excluded.checked_at",
        params![hash, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn test_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::store::migrate(&mut conn).unwrap();
        conn
    }

    #[test]
    fn test_caching_and_ttl() {
        let conn = test_conn();
        let hash = "abcde12345";

        // Starts as false
        assert!(!is_cached_no_match(&conn, hash, 60).unwrap());

        // Cache it
        record_no_match(&conn, hash).unwrap();

        // Now should be true under a positive TTL
        assert!(is_cached_no_match(&conn, hash, 60).unwrap());

        // Should be false if TTL is 0
        assert!(!is_cached_no_match(&conn, hash, 0).unwrap());
    }

    #[test]
    fn test_overwrite_on_conflict() {
        let conn = test_conn();
        let hash = "abcde12345";

        record_no_match(&conn, hash).unwrap();
        let first_checked_at: i64 = conn
            .query_row(
                "SELECT checked_at FROM ra_no_match WHERE hash = ?1",
                params![hash],
                |row| row.get(0),
            )
            .unwrap();

        // Recording again should succeed and update/keep it valid
        record_no_match(&conn, hash).unwrap();
        let second_checked_at: i64 = conn
            .query_row(
                "SELECT checked_at FROM ra_no_match WHERE hash = ?1",
                params![hash],
                |row| row.get(0),
            )
            .unwrap();

        assert!(second_checked_at >= first_checked_at);
    }
}
