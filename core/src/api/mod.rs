//! The engine facade — the only surface shells and FFI layers may use.

use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::events::Event;
use crate::launch::{self, EmulatorRow, LaunchPlan, ProfileRow};
use crate::media::{self, MediaRow, MediaStats};
use crate::metadata::gamelist::{self, GamelistImportStats};
use crate::scan::{self, ScanSummary};
use crate::systems::{self, SystemDef};
use crate::Result;

pub struct Engine {
    db: Db,
    systems: Vec<SystemDef>,
    /// Thumbnail cache next to the DB file; None for in-memory engines
    /// (discovery still works, thumbnailing is skipped).
    media_cache_dir: Option<PathBuf>,
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
        let cache = std::path::absolute(db_path)
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("relic-media-cache")));
        Self::init(db, cache)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Db::open_in_memory()?, None)
    }

    fn init(db: Db, media_cache_dir: Option<PathBuf>) -> Result<Self> {
        let defs = systems::builtin_systems()?;
        for def in &defs {
            db.conn().execute(
                "INSERT INTO systems (slug, name, sort_order, extensions) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(slug) DO UPDATE SET
                   name=excluded.name, sort_order=excluded.sort_order, extensions=excluded.extensions",
                rusqlite::params![def.slug, def.name, def.sort_order, def.extensions.join(",")],
            )?;
        }
        Ok(Self {
            db,
            systems: defs,
            media_cache_dir,
        })
    }

    pub fn version(&self) -> &'static str {
        crate::version()
    }

    /// Register a library root (idempotent on the path), returning its id.
    /// The root is stored absolute so launch commands work regardless of the
    /// working directory the shell/CLI later runs from.
    pub fn add_library(&mut self, root: &Path, name: &str) -> Result<i64> {
        let root = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
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

    /// Import every `<root>/<slug>/gamelist.xml` in a library. A gamelist
    /// that fails to parse is skipped with a Warning event; matched entries
    /// enrich `metadata` and canonical names (scan first, then import).
    pub fn import_gamelists(
        &mut self,
        library_id: i64,
        sink: &mut dyn FnMut(Event),
    ) -> Result<GamelistImportStats> {
        let root: String = self
            .db
            .conn()
            .query_row(
                "SELECT root_uri FROM libraries WHERE id=?1",
                [library_id],
                |r| r.get(0),
            )
            .map_err(|_| crate::Error::LibraryNotFound(library_id))?;

        let slugs: Vec<String> = self.systems.iter().map(|s| s.slug.clone()).collect();
        let mut total = GamelistImportStats::default();
        for slug in slugs {
            let path = PathBuf::from(&root).join(&slug).join("gamelist.xml");
            if !path.is_file() {
                continue;
            }
            let xml = std::fs::read_to_string(&path).map_err(|e| crate::Error::Io {
                path: path.clone(),
                source: e,
            })?;
            match gamelist::parse_gamelist(&xml) {
                Ok(entries) => {
                    let stats = gamelist::import_gamelist(
                        &mut self.db,
                        library_id,
                        &slug,
                        &slug,
                        &entries,
                    )?;
                    total.matched += stats.matched;
                    total.unmatched += stats.unmatched;
                    if stats.matched > 0 {
                        if let Ok(system_id) = self.db.conn().query_row(
                            "SELECT id FROM systems WHERE slug=?1",
                            [slug.as_str()],
                            |r| r.get(0),
                        ) {
                            sink(Event::GamesChanged { system_id });
                        }
                    }
                }
                Err(e) => sink(Event::Warning {
                    code: "gamelist.parse".into(),
                    context: format!("{}: {e}", path.display()),
                }),
            }
        }
        Ok(total)
    }

    /// Discover local artwork for a library (docs/media-conventions.md) and
    /// refresh the thumbnail cache. Scan first; discovery is per indexed ROM.
    pub fn refresh_media(
        &mut self,
        library_id: i64,
        sink: &mut dyn FnMut(Event),
    ) -> Result<MediaStats> {
        let root: String = self
            .db
            .conn()
            .query_row(
                "SELECT root_uri FROM libraries WHERE id=?1",
                [library_id],
                |r| r.get(0),
            )
            .map_err(|_| crate::Error::LibraryNotFound(library_id))?;
        let cache = self.media_cache_dir.clone();
        media::refresh_media(
            &mut self.db,
            library_id,
            &PathBuf::from(root),
            cache.as_deref(),
            sink,
        )
    }

    pub fn game_media(&self, game_id: i64) -> Result<Vec<MediaRow>> {
        media::media_for_game(&self.db, game_id)
    }

    /// Fill missing CRC32/MD5 for up to `limit` indexed files (lazy hashing,
    /// PLAN.md §4.2). Returns counts; call repeatedly until `hashed == 0`.
    pub fn hash_pending(
        &mut self,
        library_id: Option<i64>,
        limit: usize,
        sink: &mut dyn FnMut(Event),
    ) -> Result<crate::scan::hash::HashStats> {
        crate::scan::hash::hash_pending(&mut self.db, library_id, limit, sink)
    }

    /// Absolute path of a cached thumbnail, if the engine has a cache dir.
    pub fn thumbnail_path(&self, cache_hash: &str) -> Option<PathBuf> {
        if cache_hash.len() < 2 {
            return None;
        }
        self.media_cache_dir
            .as_ref()
            .map(|d| d.join(&cache_hash[..2]).join(format!("{cache_hash}.png")))
    }

    /// Register an emulator for the current platform, returning its id.
    pub fn add_emulator(&mut self, name: &str, exec: &str) -> Result<i64> {
        launch::add_emulator(&self.db, name, launch::current_platform(), exec)
    }

    pub fn list_emulators(&self) -> Result<Vec<EmulatorRow>> {
        launch::list_emulators(&self.db)
    }

    /// Attach a launch profile (emulator by name, system by slug). Higher
    /// priority wins when several profiles exist for one system.
    pub fn add_launch_profile(
        &mut self,
        emulator_name: &str,
        system_slug: &str,
        arg_template: &str,
        priority: i64,
    ) -> Result<i64> {
        let emulator_id = launch::emulator_id_by_name(&self.db, emulator_name)?;
        let system_id: i64 = self
            .db
            .conn()
            .query_row("SELECT id FROM systems WHERE slug=?1", [system_slug], |r| {
                r.get(0)
            })
            .map_err(|_| crate::Error::UnknownSystem(system_slug.to_string()))?;
        launch::add_profile(&self.db, emulator_id, system_id, arg_template, priority)
    }

    pub fn list_launch_profiles(&self) -> Result<Vec<ProfileRow>> {
        launch::list_profiles(&self.db)
    }

    /// Resolve a game to its concrete exec + argv without running anything.
    pub fn resolve_launch(&self, game_id: i64) -> Result<LaunchPlan> {
        launch::resolve(&self.db, game_id, &self.systems)
    }

    /// Launch a game and block until the emulator exits, recording the play
    /// session and streaming LaunchStarted/LaunchEnded through `sink`.
    pub fn launch(&mut self, game_id: i64, sink: &mut dyn FnMut(Event)) -> Result<i64> {
        let plan = launch::resolve(&self.db, game_id, &self.systems)?;
        launch::run_blocking(&self.db, &plan, sink)
    }

    /// Health check used by `relic-cli doctor` and shells on startup.
    pub fn integrity_check(&self) -> Result<bool> {
        self.db.integrity_check()
    }

    /// Games with at least one completed play session, most recently played
    /// first (PLAN.md §5 "Recently played / most played").
    pub fn recently_played(&self, limit: usize) -> Result<Vec<crate::stats::GameStats>> {
        crate::stats::recently_played(&self.db, limit)
    }

    /// Games with at least one completed play session, most total playtime
    /// first (PLAN.md §5 "Recently played / most played").
    pub fn most_played(&self, limit: usize) -> Result<Vec<crate::stats::GameStats>> {
        crate::stats::most_played(&self.db, limit)
    }

    /// `(completed session count, total seconds played)` across the library.
    pub fn play_totals(&self) -> Result<(i64, i64)> {
        crate::stats::totals(&self.db)
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
    fn launch_resolution_and_session_recording() {
        let lib = fake_library();
        let mut engine = Engine::open_in_memory().unwrap();
        let id = engine.add_library(lib.path(), "test").unwrap();
        engine.scan(id, &mut |_| {}).unwrap();
        let game_id = engine.query_games(Some("snes"), None).unwrap()[0].id;

        // No profile yet → specific error.
        assert!(matches!(
            engine.resolve_launch(game_id),
            Err(crate::Error::NoLaunchProfile(_))
        ));

        // Portable no-op "emulator" so run_blocking exercises a real child.
        let (exec, tpl) = if cfg!(windows) {
            ("cmd", "/C exit 0")
        } else {
            ("true", "{rom}")
        };
        engine.add_emulator("noop", exec).unwrap();
        engine.add_launch_profile("noop", "snes", tpl, 0).unwrap();

        let plan = engine.resolve_launch(game_id).unwrap();
        assert_eq!(plan.exec, exec);
        assert!(plan.rom_path.exists());

        let mut events = Vec::new();
        let session = engine.launch(game_id, &mut |e| events.push(e)).unwrap();
        assert!(session > 0);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::LaunchEnded { duration_s, .. } if *duration_s >= 0)));

        // A bad template placeholder is rejected at configuration time.
        assert!(engine
            .add_launch_profile("noop", "snes", "{bogus}", 1)
            .is_err());
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
