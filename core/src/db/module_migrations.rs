//! Generic migration runner for optional modules (PLAN.md §7;
//! `docs/retroachievements-design.md` §4.1 documents the rationale — written
//! against the RA module, reused here by `relic-scraper` since it is the
//! second module needing it).
//!
//! Modules own their tables in the same SQLite file as the core schema but
//! are versioned independently, through a `settings` row keyed
//! `module.<name>.schema_version` — never `PRAGMA user_version`, which is
//! reserved for the core schema (hard rule 5: append-only migrations). A
//! module that is fully removed drops its own tables and this settings row;
//! the core schema is untouched.

use rusqlite::{params, Connection, OptionalExtension};

/// Bring a module's schema up to date by applying `migrations[current..]` in
/// order, one transaction per step, and return the resulting version (==
/// `migrations.len()` on success). Migrations are append-only, same
/// discipline as the core schema: never edit a shipped entry, only append.
///
/// Creates `settings` if it does not already exist so this helper also works
/// against a bare connection in module-only tests, without bootstrapping the
/// full core schema.
pub fn apply_module_migrations(
    conn: &mut Connection,
    module_name: &str,
    migrations: &[&str],
) -> rusqlite::Result<i64> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )?;

    let key = format!("module.{module_name}.schema_version");
    let mut version: i64 = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let target = migrations.len() as i64;
    while version < target {
        let tx = conn.transaction()?;
        tx.execute_batch(migrations[version as usize])?;
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, (version + 1).to_string()],
        )?;
        tx.commit()?;
        version += 1;
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_migrations_in_order_and_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        let migrations: &[&str] = &[
            "CREATE TABLE demo_a (id INTEGER PRIMARY KEY);",
            "CREATE TABLE demo_b (id INTEGER PRIMARY KEY);",
        ];

        let version = apply_module_migrations(&mut conn, "demo", migrations).unwrap();
        assert_eq!(version, 2);
        conn.execute("INSERT INTO demo_a (id) VALUES (1)", [])
            .unwrap();
        conn.execute("INSERT INTO demo_b (id) VALUES (1)", [])
            .unwrap();

        // Re-running with the same list must not re-apply anything (tables
        // already exist; a re-run would fail on CREATE TABLE if it did).
        let version = apply_module_migrations(&mut conn, "demo", migrations).unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn appends_new_migrations_without_touching_settled_ones() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_module_migrations(
            &mut conn,
            "demo",
            &["CREATE TABLE demo_a (id INTEGER PRIMARY KEY);"],
        )
        .unwrap();
        conn.execute("INSERT INTO demo_a (id) VALUES (42)", [])
            .unwrap();

        let version = apply_module_migrations(
            &mut conn,
            "demo",
            &[
                "CREATE TABLE demo_a (id INTEGER PRIMARY KEY);",
                "CREATE TABLE demo_b (id INTEGER PRIMARY KEY);",
            ],
        )
        .unwrap();
        assert_eq!(version, 2);

        let value: i64 = conn
            .query_row("SELECT id FROM demo_a", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            value, 42,
            "existing rows in an already-migrated table must survive"
        );
    }

    #[test]
    fn different_modules_track_versions_independently() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_module_migrations(
            &mut conn,
            "alpha",
            &["CREATE TABLE alpha_t (id INTEGER PRIMARY KEY);"],
        )
        .unwrap();
        let beta_version = apply_module_migrations(
            &mut conn,
            "beta",
            &["CREATE TABLE beta_t (id INTEGER PRIMARY KEY);"],
        )
        .unwrap();
        assert_eq!(beta_version, 1);
    }
}
