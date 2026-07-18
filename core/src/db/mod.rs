//! SQLite connection handling and versioned migrations.
//!
//! Rules (PLAN.md §2.3):
//! - WAL mode, foreign keys on.
//! - Scanned tables are disposable; `user_data`, `collections`, `play_sessions`
//!   and `settings` are precious and must survive any rescan or migration.
//! - Migrations are append-only SQL files embedded at compile time.

use std::path::Path;

use rusqlite::Connection;

use crate::{Error, Result};

/// Ordered, append-only migration list. Index + 1 == resulting `user_version`.
const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_init.sql")];

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (creating if needed) the library database and bring it to the
    /// latest schema version.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// In-memory database, used by tests and `relic-cli doctor`.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<()> {
        let supported = MIGRATIONS.len() as i64;
        let mut version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > supported {
            return Err(Error::SchemaTooNew {
                found: version,
                supported,
            });
        }
        while version < supported {
            let tx = self.conn.transaction()?;
            tx.execute_batch(MIGRATIONS[version as usize])?;
            tx.pragma_update(None, "user_version", version + 1)?;
            tx.commit()?;
            version += 1;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    /// Quick corruption check, run on every open by the shells.
    pub fn integrity_check(&self) -> Result<bool> {
        let status: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        Ok(status == "ok")
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_fresh_db_to_latest() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        assert!(db.integrity_check().unwrap());
    }

    #[test]
    fn reopen_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relic.db");
        drop(Db::open(&path).unwrap());
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
    }
}
