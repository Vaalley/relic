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

#[derive(uniffi::Record)]
pub struct PendingMatchInfo {
    pub game_id: i64,
    pub provider_id: String,
    pub external_id: String,
    pub confidence: String,
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
    /// A second connection to the same db file, owned by `relic-scraper`'s
    /// module tables (versioned independently via
    /// `relic_core::db::apply_module_migrations`) — the same pattern
    /// `apps/desktop` uses directly since it links `relic-core`/
    /// `relic-scraper` natively; this crate needs its own connection because
    /// `Engine` doesn't expose its internal one.
    scraper_conn: Mutex<rusqlite::Connection>,
}

impl RelicEngine {
    fn with_engine<T>(
        &self,
        f: impl FnOnce(&mut Engine) -> relic_core::Result<T>,
    ) -> Result<T, RelicError> {
        let mut guard = self.inner.lock().map_err(|_| RelicError::Poisoned)?;
        f(&mut guard).map_err(RelicError::from)
    }

    fn with_scraper<T>(
        &self,
        f: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>,
    ) -> Result<T, RelicError> {
        let guard = self.scraper_conn.lock().map_err(|_| RelicError::Poisoned)?;
        f(&guard).map_err(|e| RelicError::Engine { msg: e.to_string() })
    }
}

#[uniffi::export]
impl RelicEngine {
    /// Open (or create) the library database at `db_path`.
    #[uniffi::constructor]
    pub fn open(db_path: String) -> Result<std::sync::Arc<Self>, RelicError> {
        let engine = Engine::open(std::path::Path::new(&db_path))?;
        let mut scraper_conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| RelicError::Engine { msg: e.to_string() })?;
        relic_scraper::migrate(&mut scraper_conn)
            .map_err(|e| RelicError::Engine { msg: e.to_string() })?;
        Ok(std::sync::Arc::new(Self {
            inner: Mutex::new(engine),
            scraper_conn: Mutex::new(scraper_conn),
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

    /// Record a play session's start without spawning a process — the
    /// Android shell fires an Intent instead (PLAN.md §4.5,
    /// `apps/android/.../IntentLauncher.kt`). Pair with `end_play_session`
    /// once the shell detects the game returned control.
    pub fn start_play_session(&self, game_id: i64) -> Result<i64, RelicError> {
        self.with_engine(|e| e.start_play_session(game_id))
    }

    /// Record a play session's end, returning its duration in seconds.
    pub fn end_play_session(&self, session_id: i64) -> Result<i64, RelicError> {
        self.with_engine(|e| e.end_play_session(session_id))
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

    /// Scraper matches not yet confirmed by the user (PLAN.md §7.1
    /// "confirmation UI for low-confidence matches"). Populated by running
    /// `relic-cli scrape` against this same db file.
    pub fn scraper_pending_matches(&self) -> Result<Vec<PendingMatchInfo>, RelicError> {
        self.with_scraper(|conn| {
            Ok(relic_scraper::pending_matches(conn)?
                .into_iter()
                .map(|p| PendingMatchInfo {
                    game_id: p.game_id,
                    provider_id: p.provider_id,
                    external_id: p.external_id,
                    confidence: p.confidence.as_str().to_string(),
                })
                .collect())
        })
    }

    pub fn scraper_confirm_match(
        &self,
        game_id: i64,
        provider_id: String,
    ) -> Result<(), RelicError> {
        self.with_scraper(|conn| relic_scraper::confirm_match(conn, game_id, &provider_id))
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

/// One Android intent template that's a launch candidate for a system
/// (docs/android-intents.md), enough for a shell to build a launch-profile
/// picker without parsing TOML itself.
#[derive(uniffi::Record)]
pub struct IntentTemplateInfo {
    pub id: String,
    pub display_name: String,
    pub package: String,
}

/// One resolved `[[extras]]` entry — ready for `Intent.putExtra` via the
/// overload matching `extra_type` (`"string"`, `"bool"`, or `"int"`).
#[derive(uniffi::Record)]
pub struct ResolvedExtraInfo {
    pub name: String,
    pub extra_type: String,
    pub value: String,
}

/// A template fully merged with its `per_system` override and with every
/// placeholder substituted (docs/android-intents.md §5) — everything a shell
/// needs to build and fire one explicit `Intent`. `data_mode` is `"data"`,
/// `"extra"`, or `"none"`.
#[derive(uniffi::Record)]
pub struct ResolvedIntentInfo {
    pub package: String,
    pub activity: String,
    pub action: String,
    pub data_mode: String,
    pub data_extra_name: Option<String>,
    pub data_mime_type: Option<String>,
    pub extras: Vec<ResolvedExtraInfo>,
    pub flags: Vec<String>,
}

/// Built-in intent templates that are launch candidates for `system_slug`
/// (`relic_core::intents::applies_to`), in the shipped `BUILTIN` order —
/// RetroArch first, then standalones. A shell tries each in order, picking
/// the first whose `package` is installed.
#[uniffi::export]
pub fn intent_templates_for_system(system_slug: String) -> Vec<IntentTemplateInfo> {
    relic_core::intents::builtin_intents()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, t)| relic_core::intents::applies_to(t, &system_slug))
        .map(|(stem, t)| IntentTemplateInfo {
            id: stem,
            display_name: t.display_name,
            package: t.package,
        })
        .collect()
}

/// Resolve `template_id` against `system_slug`, substituting `rom_uri`,
/// `rom_path`, and `core` (docs/android-intents.md §4.3). `core` must already
/// be the full path a RetroArch-family template expects (e.g.
/// `/data/data/<pkg>/cores/<stem>_libretro_android.so>`, built by the caller
/// from [`RelicEngine::system_default_core`] — this module has no notion of
/// Android package layout). Returns `None` if `template_id` doesn't match a
/// built-in template.
#[uniffi::export]
pub fn resolve_intent(
    template_id: String,
    system_slug: String,
    rom_uri: String,
    rom_path: String,
    core: Option<String>,
) -> Option<ResolvedIntentInfo> {
    let (_, template) = relic_core::intents::builtin_intents()
        .unwrap_or_default()
        .into_iter()
        .find(|(stem, _)| *stem == template_id)?;

    let ctx = relic_core::intents::LaunchContext {
        rom_uri: &rom_uri,
        rom_path: &rom_path,
        core: core.as_deref(),
    };
    let resolved = relic_core::intents::resolve(&template, &system_slug, &ctx);

    Some(ResolvedIntentInfo {
        package: resolved.package,
        activity: resolved.activity,
        action: resolved.action,
        data_mode: format!("{:?}", resolved.data_mode).to_lowercase(),
        data_extra_name: resolved.data_extra_name,
        data_mime_type: resolved.data_mime_type,
        extras: resolved
            .extras
            .into_iter()
            .map(|e| ResolvedExtraInfo {
                name: e.name,
                extra_type: format!("{:?}", e.extra_type).to_lowercase(),
                value: e.value,
            })
            .collect(),
        flags: resolved.flags,
    })
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

        // Session bookkeeping without a spawned process (Android launch path).
        let session_id = engine.start_play_session(games[0].id).unwrap();
        let duration_s = engine.end_play_session(session_id).unwrap();
        assert!(duration_s >= 0);
        assert_eq!(engine.play_totals().unwrap().sessions, 1);

        // Gamelist export should succeed on the scanned library.
        assert!(engine.export_gamelists(lib).is_ok());

        // Scraper surface: relic-cli's `scrape` is what actually populates
        // matches (via a raw connection to the same db file, same as here);
        // this exercises the read/confirm side the FFI exposes.
        assert!(engine.scraper_pending_matches().unwrap().is_empty());
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            relic_scraper::save_match(
                &conn,
                games[0].id,
                &relic_scraper::MatchResult {
                    provider_id: "screenscraper",
                    candidate: relic_scraper::Candidate {
                        external_id: "42".into(),
                        name: "Game".into(),
                        system_slug: "snes".into(),
                    },
                    confidence: relic_scraper::Confidence::Low,
                },
            )
            .unwrap();
        }
        let pending = engine.scraper_pending_matches().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].provider_id, "screenscraper");
        engine
            .scraper_confirm_match(games[0].id, "screenscraper".into())
            .unwrap();
        assert!(engine.scraper_pending_matches().unwrap().is_empty());
    }

    #[test]
    fn intent_templates_for_system_filters_and_orders_by_builtin() {
        let snes = intent_templates_for_system("snes".to_string());
        assert!(snes.iter().any(|t| t.id == "retroarch"));
        assert!(!snes.iter().any(|t| t.id == "duckstation"));

        let psx = intent_templates_for_system("psx".to_string());
        assert!(psx.iter().any(|t| t.id == "retroarch"));
        assert!(psx.iter().any(|t| t.id == "duckstation"));
        // RetroArch (either package alias, both first in BUILTIN) should
        // come before standalones.
        assert!(psx[0].id.starts_with("retroarch"));
    }

    #[test]
    fn resolve_intent_substitutes_and_reports_unknown_template() {
        assert!(resolve_intent(
            "not-a-real-template".to_string(),
            "snes".to_string(),
            "content://x".to_string(),
            "snes/game.zip".to_string(),
            None,
        )
        .is_none());

        let resolved = resolve_intent(
            "retroarch".to_string(),
            "snes".to_string(),
            "content://relic/rom".to_string(),
            "snes/game.zip".to_string(),
            Some("/data/data/com.retroarch/cores/snes9x_libretro_android.so".to_string()),
        )
        .expect("retroarch is a built-in template");

        assert_eq!(resolved.package, "com.retroarch");
        assert_eq!(resolved.data_mode, "extra");
        let rom_extra = resolved.extras.iter().find(|e| e.name == "ROM").unwrap();
        assert_eq!(rom_extra.value, "content://relic/rom");
        let core_extra = resolved
            .extras
            .iter()
            .find(|e| e.name == "LIBRETRO")
            .unwrap();
        assert_eq!(
            core_extra.value,
            "/data/data/com.retroarch/cores/snes9x_libretro_android.so"
        );
        assert!(resolved
            .flags
            .iter()
            .any(|f| f == "FLAG_GRANT_READ_URI_PERMISSION"));
    }
}
