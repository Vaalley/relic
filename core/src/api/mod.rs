//! The engine facade — the only surface shells and FFI layers may use.

use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::events::Event;
use crate::scan::{self, ScanSummary};
use crate::systems::{self, SystemDef};
use crate::Result;

pub struct Engine {
    db: Db,
    systems: Vec<SystemDef>,
}

#[derive(Debug, Clone)]
pub struct SystemRow {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub game_count: i64,
}

#[derive(Debug, Clone)]
pub struct GameRow {
    pub id: i64,
    pub system_slug: String,
    pub name: String,
    pub favorite: bool,
    pub rel_path: Option<String>,
}

impl Engine {
    /// Open a library database, migrating and seeding the systems registry.
    pub fn open(db_path: &Path) -> Result<Self> {
        let db = Db::open(db_path)?;
        Self::init(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Db::open_in_memory()?)
    }

    fn init(db: Db) -> Result<Self> {
        let defs = systems::builtin_systems()?;
        for def in &defs {
            db.conn().execute(
                "INSERT INTO systems (slug, name, sort_order, extensions) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(slug) DO UPDATE SET
                   name=excluded.name, sort_order=excluded.sort_order, extensions=excluded.extensions",
                rusqlite::params![def.slug, def.name, def.sort_order, def.extensions.join(",")],
            )?;
        }
        Ok(Self { db, systems: defs })
    }

    pub fn version(&self) -> &'static str {
        crate::version()
    }

    /// Register a library root (idempotent on the path), returning its id.
    pub fn add_library(&mut self, root: &Path, name: &str) -> Result<i64> {
        let uri = root.to_string_lossy().replace('\\', "/");
        self.db.conn().execute(
            "INSERT INTO libraries (root_uri, name) VALUES (?1, ?2)
             ON CONFLICT(root_uri) DO UPDATE SET name=excluded.name",
            rusqlite::params![uri, name],
        )?;
        Ok(self
            .db
            .conn()
            .query_row("SELECT id FROM libraries WHERE root_uri=?1", [uri], |r| {
                r.get(0)
            })?)
    }

    /// Incrementally (re)scan one library, streaming progress into `sink`.
    pub fn scan(&mut self, library_id: i64, sink: &mut dyn FnMut(Event)) -> Result<ScanSummary> {
        let root: String = self
            .db
            .conn()
            .query_row(
                "SELECT root_uri FROM libraries WHERE id=?1",
                [library_id],
                |r| r.get(0),
            )
            .map_err(|_| crate::Error::LibraryNotFound(library_id))?;
        let systems = self.systems.clone();
        scan::scan_library(
            &mut self.db,
            library_id,
            &PathBuf::from(root),
            &systems,
            sink,
        )
    }

    pub fn list_systems(&self) -> Result<Vec<SystemRow>> {
        let mut stmt = self.db.conn().prepare(
            "SELECT s.id, s.slug, s.name, COUNT(g.id)
             FROM systems s LEFT JOIN games g ON g.system_id = s.id
             GROUP BY s.id ORDER BY s.sort_order, s.name",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(SystemRow {
                    id: r.get(0)?,
                    slug: r.get(1)?,
                    name: r.get(2)?,
                    game_count: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// List games, optionally filtered by system slug and/or an FTS query.
    pub fn query_games(&self, system: Option<&str>, search: Option<&str>) -> Result<Vec<GameRow>> {
        let sql = "
            SELECT g.id, s.slug,
                   COALESCE(u.custom_name, g.canonical_name),
                   COALESCE(u.favorite, 0),
                   (SELECT rel_path FROM files f WHERE f.game_id = g.id LIMIT 1)
            FROM games g
            JOIN systems s ON s.id = g.system_id
            LEFT JOIN user_data u ON u.game_id = g.id
            WHERE COALESCE(u.hidden, 0) = 0
              AND (?1 IS NULL OR s.slug = ?1)
              AND (?2 IS NULL OR g.id IN (SELECT rowid FROM games_fts WHERE games_fts MATCH ?2))
            ORDER BY g.sort_name";
        let mut stmt = self.db.conn().prepare(sql)?;
        let rows = stmt
            .query_map(rusqlite::params![system, search], |r| {
                Ok(GameRow {
                    id: r.get(0)?,
                    system_slug: r.get(1)?,
                    name: r.get(2)?,
                    favorite: r.get::<_, i64>(3)? != 0,
                    rel_path: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_favorite(&mut self, game_id: i64, favorite: bool) -> Result<()> {
        self.db.conn().execute(
            "INSERT INTO user_data (game_id, favorite) VALUES (?1, ?2)
             ON CONFLICT(game_id) DO UPDATE SET favorite=excluded.favorite",
            rusqlite::params![game_id, favorite as i64],
        )?;
        Ok(())
    }

    /// Health check used by `relic-cli doctor` and shells on startup.
    pub fn integrity_check(&self) -> Result<bool> {
        self.db.integrity_check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_library() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let snes = dir.path().join("snes");
        fs::create_dir_all(&snes).unwrap();
        fs::write(snes.join("Super Mario World (USA).sfc"), b"stub").unwrap();
        fs::write(snes.join("The Legend of Zelda - ALttP.sfc"), b"stub").unwrap();
        fs::write(snes.join("readme.txt"), b"not a rom").unwrap();
        dir
    }

    #[test]
    fn scan_indexes_matching_files_only() {
        let lib = fake_library();
        let mut engine = Engine::open_in_memory().unwrap();
        let id = engine.add_library(lib.path(), "test").unwrap();
        let summary = engine.scan(id, &mut |_| {}).unwrap();
        assert_eq!(summary.added, 2);

        let games = engine.query_games(Some("snes"), None).unwrap();
        assert_eq!(games.len(), 2);
        // "The " prefix stripped by sort key → Zelda sorts before Super Mario? No:
        // "legend of zelda..." < "super mario world" alphabetically.
        assert!(games[0].name.contains("Zelda"));
    }

    #[test]
    fn rescan_is_incremental_and_detects_removal() {
        let lib = fake_library();
        let mut engine = Engine::open_in_memory().unwrap();
        let id = engine.add_library(lib.path(), "test").unwrap();
        engine.scan(id, &mut |_| {}).unwrap();

        let summary = engine.scan(id, &mut |_| {}).unwrap();
        assert_eq!(summary.added, 0);
        assert_eq!(summary.unchanged, 2);

        fs::remove_file(lib.path().join("snes/Super Mario World (USA).sfc")).unwrap();
        let summary = engine.scan(id, &mut |_| {}).unwrap();
        assert_eq!(summary.removed, 1);
        assert_eq!(engine.query_games(Some("snes"), None).unwrap().len(), 1);
    }

    #[test]
    fn search_and_favorites() {
        let lib = fake_library();
        let mut engine = Engine::open_in_memory().unwrap();
        let id = engine.add_library(lib.path(), "test").unwrap();
        engine.scan(id, &mut |_| {}).unwrap();

        let hits = engine.query_games(None, Some("zelda")).unwrap();
        assert_eq!(hits.len(), 1);
        engine.set_favorite(hits[0].id, true).unwrap();
        let again = engine.query_games(None, Some("zelda")).unwrap();
        assert!(again[0].favorite);
    }
}
