//! UniFFI surface over relic-core (Phase 1→3, PLAN.md §2.1/§4.1).
//!
//! One interface definition, generated Kotlin/Swift bindings. The exported
//! object wraps `relic_core::api::Engine` behind a mutex because UniFFI
//! objects are shared across threads while the engine expects `&mut` for
//! commands. Blocking calls (scan) are expected to run on a background
//! dispatcher on the Kotlin side; events stream through a foreign-implemented
//! listener trait.
//!
//! Also exposes `relic-themes`' resolved design tokens (PLAN.md §6 layer 1,
//! `theme_colors`) as a free function — theming needs no engine instance.

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

#[derive(uniffi::Record)]
pub struct CollectionInfo {
    pub id: i64,
    pub name: String,
    /// `"manual"` or `"smart"`.
    pub kind: String,
}

#[derive(uniffi::Record)]
pub struct GameStatsInfo {
    pub game_id: i64,
    pub name: String,
    pub system_slug: String,
    pub play_count: i64,
    pub total_seconds: i64,
    pub last_played_at: Option<i64>,
}

#[derive(uniffi::Record)]
pub struct PlayTotals {
    pub sessions: i64,
    pub total_seconds: i64,
}

#[derive(uniffi::Record)]
pub struct DatImportStats {
    pub matched: u64,
    pub unmatched: u64,
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

    pub fn system_default_core(&self, slug: String) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        guard.system_default_core(&slug)
    }

    /// Boxart thumbnail path for a game, if one is cached — convenience for
    /// grid shells (avoids a media-row round trip per tile).
    pub fn boxart_path(&self, game_id: i64) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        let media = guard.game_media(game_id).ok()?;
        let row = media
            .iter()
            .find(|m| m.kind == "boxart" && !m.cache_hash.is_empty())?;
        guard
            .thumbnail_path(&row.cache_hash)
            .map(|p| p.to_string_lossy().into_owned())
    }

    /// Thumbnail cache path for a media hash, if a cache exists.
    pub fn thumbnail_path(&self, cache_hash: String) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        guard
            .thumbnail_path(&cache_hash)
            .map(|p| p.to_string_lossy().into_owned())
    }

    pub fn create_manual_collection(&self, name: String) -> Result<i64, RelicError> {
        self.with_engine(|e| e.create_manual_collection(&name))
    }

    /// `system`/`search`/`favorite` are the same filters `relic-cli
    /// collection-add-smart` takes; membership is computed live, not stored.
    pub fn create_smart_collection(
        &self,
        name: String,
        system: Option<String>,
        search: Option<String>,
        favorite: Option<bool>,
    ) -> Result<i64, RelicError> {
        self.with_engine(|e| {
            let query = relic_core::collections::SmartQuery {
                system,
                search,
                favorite,
            };
            e.create_smart_collection(&name, &query)
        })
    }

    pub fn list_collections(&self) -> Result<Vec<CollectionInfo>, RelicError> {
        self.with_engine(|e| {
            Ok(e.list_collections()?
                .into_iter()
                .map(|c| CollectionInfo {
                    id: c.id,
                    name: c.name,
                    kind: c.kind,
                })
                .collect())
        })
    }

    pub fn collection_games(&self, collection_id: i64) -> Result<Vec<GameInfo>, RelicError> {
        self.with_engine(|e| {
            Ok(e.collection_games(collection_id)?
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

    /// No-op (per `relic-core`) if `game_id` is already a member.
    pub fn add_to_collection(&self, collection_id: i64, game_id: i64) -> Result<(), RelicError> {
        self.with_engine(|e| e.add_to_collection(collection_id, game_id))
    }

    pub fn remove_from_collection(
        &self,
        collection_id: i64,
        game_id: i64,
    ) -> Result<(), RelicError> {
        self.with_engine(|e| e.remove_from_collection(collection_id, game_id))
    }

    /// Works for both manual and smart collections.
    pub fn delete_collection(&self, collection_id: i64) -> Result<(), RelicError> {
        self.with_engine(|e| e.delete_collection(collection_id))
    }

    /// Most recently played games, newest first.
    pub fn recently_played(&self, limit: u64) -> Result<Vec<GameStatsInfo>, RelicError> {
        self.with_engine(|e| {
            Ok(e.recently_played(limit as usize)?
                .into_iter()
                .map(|s| GameStatsInfo {
                    game_id: s.game_id,
                    name: s.name,
                    system_slug: s.system_slug,
                    play_count: s.play_count,
                    total_seconds: s.total_seconds,
                    last_played_at: s.last_played_at,
                })
                .collect())
        })
    }

    /// Top-played games by total seconds.
    pub fn most_played(&self, limit: u64) -> Result<Vec<GameStatsInfo>, RelicError> {
        self.with_engine(|e| {
            Ok(e.most_played(limit as usize)?
                .into_iter()
                .map(|s| GameStatsInfo {
                    game_id: s.game_id,
                    name: s.name,
                    system_slug: s.system_slug,
                    play_count: s.play_count,
                    total_seconds: s.total_seconds,
                    last_played_at: s.last_played_at,
                })
                .collect())
        })
    }

    /// Aggregate play stats: `(sessions, total_seconds)`.
    pub fn play_totals(&self) -> Result<PlayTotals, RelicError> {
        self.with_engine(|e| {
            let (sessions, total_seconds) = e.play_totals()?;
            Ok(PlayTotals {
                sessions,
                total_seconds,
            })
        })
    }

    /// Match a DAT XML's `<game>`/`<rom>` entries against scanned ROMs.
    pub fn import_dat(
        &self,
        system_slug: String,
        dat_xml: String,
    ) -> Result<DatImportStats, RelicError> {
        self.with_engine(|e| {
            let stats = e.import_dat(&system_slug, &dat_xml)?;
            Ok(DatImportStats {
                matched: stats.matched,
                unmatched: stats.unmatched,
            })
        })
    }

    /// Write `gamelist.xml` for `library_id`'s systems; returns bytes written.
    pub fn export_gamelists(&self, library_id: i64) -> Result<u64, RelicError> {
        self.with_engine(|e| e.export_gamelists(library_id))
    }
}

/// Resolved design tokens (PLAN.md §6 layer 1) for a shell to apply, so
/// Kotlin/Swift never duplicate `relic-themes`' resolution logic. Colors are
/// `"#rrggbb"` hex strings, same convention as the bundled default theme.
#[derive(uniffi::Record)]
pub struct ThemeColors {
    pub bg: String,
    pub surface: String,
    pub text: String,
    pub text_dim: String,
    pub accent: String,
    pub favorite: String,
    pub font_family: String,
    pub radius: i64,
}

/// Resolve the bundled default theme's tokens for `dark`/light variant.
/// Only the default theme is exposed for now — custom theme loading over
/// FFI is deferred until a shell needs to let users pick one.
#[uniffi::export]
pub fn theme_colors(dark: bool) -> ThemeColors {
    let variant = if dark {
        relic_themes::Variant::Dark
    } else {
        relic_themes::Variant::Light
    };
    let tokens = relic_themes::resolve(Some(relic_themes::default_theme()), variant);
    ThemeColors {
        bg: tokens.colors.bg,
        surface: tokens.colors.surface,
        text: tokens.colors.text,
        text_dim: tokens.colors.text_dim,
        accent: tokens.colors.accent,
        favorite: tokens.colors.favorite,
        font_family: tokens.font_family,
        radius: tokens.radius,
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
    fn theme_colors_resolves_dark_and_light_variants_distinctly() {
        let dark = theme_colors(true);
        let light = theme_colors(false);
        assert!(dark.bg.starts_with('#'));
        assert!(light.bg.starts_with('#'));
        assert_ne!(dark.bg, light.bg);
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

        let manual_id = engine.create_manual_collection("Faves".into()).unwrap();
        engine.add_to_collection(manual_id, games[0].id).unwrap();
        assert_eq!(engine.collection_games(manual_id).unwrap().len(), 1);
        engine
            .remove_from_collection(manual_id, games[0].id)
            .unwrap();
        assert!(engine.collection_games(manual_id).unwrap().is_empty());

        let smart_id = engine
            .create_smart_collection("Favorites".into(), None, None, Some(true))
            .unwrap();
        assert_eq!(engine.collection_games(smart_id).unwrap().len(), 1);

        let collections = engine.list_collections().unwrap();
        assert_eq!(collections.len(), 2);
        engine.delete_collection(manual_id).unwrap();
        engine.delete_collection(smart_id).unwrap();
        assert!(engine.list_collections().unwrap().is_empty());

        // Stats surface: fresh library has no play sessions yet.
        let totals = engine.play_totals().unwrap();
        assert_eq!(totals.sessions, 0);
        assert_eq!(totals.total_seconds, 0);
        assert!(engine.recently_played(10).unwrap().is_empty());
        assert!(engine.most_played(10).unwrap().is_empty());

        // Gamelist export should succeed on the scanned library.
        assert!(engine.export_gamelists(lib).is_ok());
    }
}
