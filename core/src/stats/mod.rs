//! Play statistics: recently played, most played, and library-wide totals
//! (Phase 1, PLAN.md §5 "Playtime tracking & per-game stats" / "Recently
//! played / most played / random \"surprise me\"").
//!
//! Sessions are recorded by [`crate::launch::run_blocking`]; this module only
//! *reads* `play_sessions`. A session counts as "completed" once `ended_at`
//! is set — in-progress sessions (emulator still running, or one that
//! crashed before the bookkeeping `UPDATE` ran) are excluded from every
//! count and total here, the same way they'd be excluded from a resume-time
//! playtime display.

use crate::db::Db;
use crate::Result;

/// One game's aggregated play history, as surfaced to shells.
#[derive(Debug, Clone)]
pub struct GameStats {
    pub game_id: i64,
    pub name: String,
    pub system_slug: String,
    pub play_count: i64,
    pub total_seconds: i64,
    pub last_played_at: Option<i64>,
}

/// Games with at least one completed session, most recently played first.
/// Hidden games (`user_data.hidden`) are skipped, matching
/// [`crate::api::Engine::query_games`].
pub(crate) fn recently_played(db: &Db, limit: usize) -> Result<Vec<GameStats>> {
    let mut stmt = db.conn().prepare(
        "SELECT g.id, COALESCE(u.custom_name, g.canonical_name), s.slug,
                COUNT(ps.id), COALESCE(SUM(ps.duration_s), 0), MAX(ps.ended_at) AS last_played_at
         FROM games g
         JOIN systems s ON s.id = g.system_id
         JOIN play_sessions ps ON ps.game_id = g.id AND ps.ended_at IS NOT NULL
         LEFT JOIN user_data u ON u.game_id = g.id
         WHERE COALESCE(u.hidden, 0) = 0
         GROUP BY g.id
         ORDER BY last_played_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit as i64], row_to_stats)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Games with at least one completed session, most total playtime first.
/// Hidden games (`user_data.hidden`) are skipped, matching
/// [`crate::api::Engine::query_games`].
pub(crate) fn most_played(db: &Db, limit: usize) -> Result<Vec<GameStats>> {
    let mut stmt = db.conn().prepare(
        "SELECT g.id, COALESCE(u.custom_name, g.canonical_name), s.slug,
                COUNT(ps.id), COALESCE(SUM(ps.duration_s), 0) AS total_seconds, MAX(ps.ended_at)
         FROM games g
         JOIN systems s ON s.id = g.system_id
         JOIN play_sessions ps ON ps.game_id = g.id AND ps.ended_at IS NOT NULL
         LEFT JOIN user_data u ON u.game_id = g.id
         WHERE COALESCE(u.hidden, 0) = 0
         GROUP BY g.id
         ORDER BY total_seconds DESC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit as i64], row_to_stats)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn row_to_stats(r: &rusqlite::Row) -> rusqlite::Result<GameStats> {
    Ok(GameStats {
        game_id: r.get(0)?,
        name: r.get(1)?,
        system_slug: r.get(2)?,
        play_count: r.get(3)?,
        total_seconds: r.get(4)?,
        last_played_at: r.get(5)?,
    })
}

/// `(completed session count, total seconds played)` across the whole
/// library, regardless of hidden status — a global counter, not a browse view.
pub(crate) fn totals(db: &Db) -> Result<(i64, i64)> {
    Ok(db.conn().query_row(
        "SELECT COUNT(*), COALESCE(SUM(duration_s), 0)
         FROM play_sessions WHERE ended_at IS NOT NULL",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seed one system (idempotent per slug) and one game, returning `(system_id, game_id)`.
    fn seed_game(db: &Db, slug: &str, canonical_name: &str) -> (i64, i64) {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO systems (slug, name, extensions) VALUES (?1, ?1, 'rom')
             ON CONFLICT(slug) DO NOTHING",
            [slug],
        )
        .unwrap();
        let system_id: i64 = conn
            .query_row("SELECT id FROM systems WHERE slug=?1", [slug], |r| r.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO games (system_id, canonical_name, sort_name) VALUES (?1, ?2, ?2)",
            rusqlite::params![system_id, canonical_name],
        )
        .unwrap();
        (system_id, conn.last_insert_rowid())
    }

    /// Insert a completed session: `duration_s = ended_at - started_at`.
    fn seed_session(db: &Db, game_id: i64, started_at: i64, ended_at: i64) {
        db.conn()
            .execute(
                "INSERT INTO play_sessions (game_id, started_at, ended_at, duration_s)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![game_id, started_at, ended_at, ended_at - started_at],
            )
            .unwrap();
    }

    /// Insert an in-progress session: no `ended_at`/`duration_s` yet.
    fn seed_in_progress_session(db: &Db, game_id: i64, started_at: i64) {
        db.conn()
            .execute(
                "INSERT INTO play_sessions (game_id, started_at) VALUES (?1, ?2)",
                rusqlite::params![game_id, started_at],
            )
            .unwrap();
    }

    fn hide(db: &Db, game_id: i64) {
        db.conn()
            .execute(
                "INSERT INTO user_data (game_id, hidden) VALUES (?1, 1)
                 ON CONFLICT(game_id) DO UPDATE SET hidden=1",
                [game_id],
            )
            .unwrap();
    }

    fn set_custom_name(db: &Db, game_id: i64, name: &str) {
        db.conn()
            .execute(
                "INSERT INTO user_data (game_id, custom_name) VALUES (?1, ?2)
                 ON CONFLICT(game_id) DO UPDATE SET custom_name=excluded.custom_name",
                rusqlite::params![game_id, name],
            )
            .unwrap();
    }

    #[test]
    fn recently_played_and_most_played_can_disagree_on_order() {
        let db = Db::open_in_memory().unwrap();
        let (_, game_a) = seed_game(&db, "snes", "Game A");
        let (_, game_b) = seed_game(&db, "snes", "Game B");

        // A: one long session, played long ago.
        seed_session(&db, game_a, 1000, 1100); // 100s, last_played_at=1100
                                               // B: two short sessions, played more recently.
        seed_session(&db, game_b, 1500, 1510); // 10s
        seed_session(&db, game_b, 2000, 2010); // 10s, last_played_at=2010

        let recent = recently_played(&db, 10).unwrap();
        assert_eq!(
            recent.iter().map(|g| g.game_id).collect::<Vec<_>>(),
            vec![game_b, game_a]
        );
        assert_eq!(recent[0].last_played_at, Some(2010));
        assert_eq!(recent[0].play_count, 2);
        assert_eq!(recent[0].total_seconds, 20);

        let most = most_played(&db, 10).unwrap();
        assert_eq!(
            most.iter().map(|g| g.game_id).collect::<Vec<_>>(),
            vec![game_a, game_b]
        );
        assert_eq!(most[0].total_seconds, 100);
        assert_eq!(most[0].play_count, 1);
    }

    #[test]
    fn recently_played_respects_limit() {
        let db = Db::open_in_memory().unwrap();
        let (_, game_a) = seed_game(&db, "snes", "Game A");
        let (_, game_b) = seed_game(&db, "snes", "Game B");
        seed_session(&db, game_a, 0, 10);
        seed_session(&db, game_b, 20, 40);

        assert_eq!(recently_played(&db, 1).unwrap().len(), 1);
        assert_eq!(recently_played(&db, 0).unwrap().len(), 0);
    }

    #[test]
    fn hidden_games_are_excluded_from_both_views() {
        let db = Db::open_in_memory().unwrap();
        let (_, visible) = seed_game(&db, "snes", "Visible");
        let (_, hidden_game) = seed_game(&db, "snes", "Hidden");
        seed_session(&db, visible, 0, 10);
        seed_session(&db, hidden_game, 0, 1000); // would otherwise win most_played
        hide(&db, hidden_game);

        let recent = recently_played(&db, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].game_id, visible);

        let most = most_played(&db, 10).unwrap();
        assert_eq!(most.len(), 1);
        assert_eq!(most[0].game_id, visible);
    }

    #[test]
    fn custom_name_wins_over_canonical_name() {
        let db = Db::open_in_memory().unwrap();
        let (_, game) = seed_game(&db, "snes", "Canonical Name");
        seed_session(&db, game, 0, 10);
        set_custom_name(&db, game, "My Nickname");

        let recent = recently_played(&db, 10).unwrap();
        assert_eq!(recent[0].name, "My Nickname");
        assert_eq!(recent[0].system_slug, "snes");
    }

    #[test]
    fn in_progress_sessions_are_excluded_from_counts_and_totals() {
        let db = Db::open_in_memory().unwrap();
        let (_, finished_game) = seed_game(&db, "snes", "Finished");
        let (_, running_only) = seed_game(&db, "snes", "Still Running");
        seed_session(&db, finished_game, 0, 30);
        seed_in_progress_session(&db, running_only, 100);
        // Extra in-progress session on the finished game shouldn't add to its
        // count/total either.
        seed_in_progress_session(&db, finished_game, 200);

        let recent = recently_played(&db, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].game_id, finished_game);
        assert_eq!(recent[0].play_count, 1);
        assert_eq!(recent[0].total_seconds, 30);

        let most = most_played(&db, 10).unwrap();
        assert_eq!(most.len(), 1);
        assert_eq!(most[0].game_id, finished_game);

        let (sessions, seconds) = totals(&db).unwrap();
        assert_eq!(sessions, 1);
        assert_eq!(seconds, 30);
    }

    #[test]
    fn totals_are_zero_for_an_empty_library() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(totals(&db).unwrap(), (0, 0));
    }
}
