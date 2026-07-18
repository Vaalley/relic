//! UniFFI surface over relic-core (Phase 1→3, PLAN.md §2.1/§4.1).
//!
//! One interface definition, generated Kotlin/Swift bindings. The exported
//! object wraps `relic_core::api::Engine` behind a mutex because UniFFI
//! objects are shared across threads while the engine expects `&mut` for
//! commands. Blocking calls (scan) are expected to run on a background
//! dispatcher on the Kotlin side; events stream through a foreign-implemented
//! listener trait.

use std::sync::Mutex;

use relic_core::api::Engine;
use relic_core::events::Event;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum RelicError {
    #[error("{msg}")]
    Engine { msg: String },
    #[error("engine lock poisoned")]
    Poisoned,
}

impl From<relic_core::Error> for RelicError {
    fn from(e: relic_core::Error) -> Self {
        RelicError::Engine { msg: e.to_string() }
    }
}

#[derive(uniffi::Record)]
pub struct SystemInfo {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub game_count: i64,
}

#[derive(uniffi::Record)]
pub struct GameInfo {
    pub id: i64,
    pub system_slug: String,
    pub name: String,
    pub favorite: bool,
    pub rel_path: Option<String>,
}

#[derive(uniffi::Record)]
pub struct ScanSummary {
    pub added: u64,
    pub removed: u64,
    pub unchanged: u64,
}

/// Implemented on the foreign side (Kotlin/Swift) to receive engine events.
#[uniffi::export(callback_interface)]
pub trait EventListener: Send + Sync {
    fn on_scan_progress(&self, done: u64, total: u64);
    fn on_warning(&self, code: String, context: String);
}

#[derive(uniffi::Object)]
pub struct RelicEngine {
    inner: Mutex<Engine>,
}

impl RelicEngine {
    fn with_engine<T>(
        &self,
        f: impl FnOnce(&mut Engine) -> relic_core::Result<T>,
    ) -> Result<T, RelicError> {
        let mut guard = self.inner.lock().map_err(|_| RelicError::Poisoned)?;
        f(&mut guard).map_err(RelicError::from)
    }
}

#[uniffi::export]
impl RelicEngine {
    /// Open (or create) the library database at `db_path`.
    #[uniffi::constructor]
    pub fn open(db_path: String) -> Result<std::sync::Arc<Self>, RelicError> {
        let engine = Engine::open(std::path::Path::new(&db_path))?;
        Ok(std::sync::Arc::new(Self {
            inner: Mutex::new(engine),
        }))
    }

    pub fn version(&self) -> String {
        relic_core::version().to_string()
    }

    pub fn list_systems(&self) -> Result<Vec<SystemInfo>, RelicError> {
        self.with_engine(|e| {
            Ok(e.list_systems()?
                .into_iter()
                .map(|s| SystemInfo {
                    id: s.id,
                    slug: s.slug,
                    name: s.name,
                    game_count: s.game_count,
                })
                .collect())
        })
    }

    pub fn query_games(
        &self,
        system: Option<String>,
        search: Option<String>,
    ) -> Result<Vec<GameInfo>, RelicError> {
        self.with_engine(|e| {
            Ok(e.query_games(system.as_deref(), search.as_deref())?
                .into_iter()
                .map(|g| GameInfo {
                    id: g.id,
                    system_slug: g.system_slug,
                    name: g.name,
                    favorite: g.favorite,
                    rel_path: g.rel_path,
                })
                .collect())
        })
    }

    pub fn add_library(&self, root: String, name: String) -> Result<i64, RelicError> {
        self.with_engine(|e| e.add_library(std::path::Path::new(&root), &name))
    }

    /// Blocking; call from a background dispatcher. Progress and warnings
    /// stream through `listener` while the scan runs.
    pub fn scan(
        &self,
        library_id: i64,
        listener: Box<dyn EventListener>,
    ) -> Result<ScanSummary, RelicError> {
        self.with_engine(|e| {
            let summary = e.scan(library_id, &mut |event| match event {
                Event::ScanProgress { done, total, .. } => listener.on_scan_progress(done, total),
                Event::Warning { code, context } => listener.on_warning(code, context),
                _ => {}
            })?;
            Ok(ScanSummary {
                added: summary.added,
                removed: summary.removed,
                unchanged: summary.unchanged,
            })
        })
    }

    pub fn import_gamelists(&self, library_id: i64) -> Result<u64, RelicError> {
        self.with_engine(|e| Ok(e.import_gamelists(library_id, &mut |_| {})?.matched))
    }

    pub fn refresh_media(&self, library_id: i64) -> Result<u64, RelicError> {
        self.with_engine(|e| Ok(e.refresh_media(library_id, &mut |_| {})?.discovered))
    }

    pub fn set_favorite(&self, game_id: i64, favorite: bool) -> Result<(), RelicError> {
        self.with_engine(|e| e.set_favorite(game_id, favorite))
    }

    /// Thumbnail cache path for a media hash, if a cache exists.
    pub fn thumbnail_path(&self, cache_hash: String) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        guard
            .thumbnail_path(&cache_hash)
            .map(|p| p.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NullListener;
    impl EventListener for NullListener {
        fn on_scan_progress(&self, _done: u64, _total: u64) {}
        fn on_warning(&self, _code: String, _context: String) {}
    }

    #[test]
    fn ffi_surface_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let snes = dir.path().join("roms/snes");
        std::fs::create_dir_all(&snes).unwrap();
        std::fs::write(snes.join("Game.sfc"), b"x").unwrap();

        let db = dir.path().join("relic.db");
        let engine = RelicEngine::open(db.to_string_lossy().into_owned()).unwrap();
        assert!(!engine.version().is_empty());
        assert!(engine.list_systems().unwrap().len() >= 16);

        let lib = engine
            .add_library(
                dir.path().join("roms").to_string_lossy().into_owned(),
                "test".into(),
            )
            .unwrap();
        let summary = engine.scan(lib, Box::new(NullListener)).unwrap();
        assert_eq!(summary.added, 1);
        let games = engine.query_games(Some("snes".into()), None).unwrap();
        assert_eq!(games.len(), 1);
        engine.set_favorite(games[0].id, true).unwrap();
        assert!(engine.query_games(None, None).unwrap()[0].favorite);
    }
}
