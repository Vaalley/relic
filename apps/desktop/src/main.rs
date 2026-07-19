//! Relic desktop shell (Phase 2, early).
//!
//! UI stack decision (ADR-002, `docs/adr/0002-desktop-ui-stack.md`) is settled:
//! Slint. This is a system/game browser skeleton over real `Engine` data;
//! gamepad input is the one PLAN.md Phase 2 exit criterion still missing.
//! Colors resolve through `relic-themes`' default theme (PLAN.md section 6
//! layer 1) into the `Palette` Slint global. Emulator auto-detection
//! (`emulator_detect`) covers the "detect emulators" half of the first-run
//! wizard requirement.
//!
//! The library database is a real file under the OS data dir (persists
//! across launches — added libraries stay added). In debug builds, if that
//! database has no games yet, `fixtures/mini` is scanned in as a one-time
//! seed so the views aren't empty before a real folder is added.

slint::include_modules!();

mod emulator_detect;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use relic_core::api::{Engine, GameRow};
use relic_core::events::Event;
use slint::{ModelRc, StandardListViewItem, VecModel};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mini")
}

fn app_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("relic")
        .join("library.db")
}

/// Parse a `relic-themes`-emitted `"#rrggbb"` string into a Slint color.
/// Never panics: malformed input (which `relic-themes` never produces, per
/// its "deterministic and never raises" guarantee) falls back to black.
fn parse_hex_color(s: &str) -> slint::Color {
    let hex = s.strip_prefix('#').unwrap_or(s);
    let parse_channel = |range| u8::from_str_radix(&hex[range], 16).ok();
    if hex.len() == 6 {
        if let (Some(r), Some(g), Some(b)) = (
            parse_channel(0..2),
            parse_channel(2..4),
            parse_channel(4..6),
        ) {
            return slint::Color::from_rgb_u8(r, g, b);
        }
    }
    slint::Color::from_rgb_u8(0, 0, 0)
}

fn main() -> Result<(), slint::PlatformError> {
    let db_path = app_db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).expect("creating the app data directory should not fail");
    }
    let mut engine = Engine::open(&db_path).expect("opening the library database should not fail");

    let has_games = engine
        .list_systems()
        .unwrap_or_default()
        .iter()
        .any(|s| s.game_count > 0);
    if cfg!(debug_assertions) && !has_games {
        let root = fixtures_root();
        if root.is_dir() {
            if let Ok(library_id) = engine.add_library(&root, "fixtures-mini") {
                let _ = engine.scan(library_id, &mut |_event| {});
            }
        }
    }

    let window = MainWindow::new()?;

    let tokens = relic_themes::resolve(
        Some(relic_themes::default_theme()),
        relic_themes::Variant::Dark,
    );
    let palette = window.global::<Palette>();
    palette.set_bg(parse_hex_color(&tokens.colors.bg));
    palette.set_surface(parse_hex_color(&tokens.colors.surface));
    palette.set_text(parse_hex_color(&tokens.colors.text));
    palette.set_text_dim(parse_hex_color(&tokens.colors.text_dim));
    palette.set_accent(parse_hex_color(&tokens.colors.accent));
    palette.set_favorite(parse_hex_color(&tokens.colors.favorite));

    window.set_status_line(format!("core {} — {}", engine.version(), db_path.display()).into());

    let mut scraper_conn = rusqlite::Connection::open(&db_path)
        .expect("opening the scraper connection should not fail");
    relic_scraper::migrate(&mut scraper_conn).expect("scraper migration should not fail");
    let scraper_conn = Rc::new(RefCell::new(scraper_conn));

    let engine = Rc::new(RefCell::new(engine));
    let system_slugs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let current_system: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    let current_games: Rc<RefCell<Vec<GameRow>>> = Rc::new(RefCell::new(Vec::new()));
    let show_stats: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let show_scraper: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let show_detail: Rc<Cell<bool>> = Rc::new(Cell::new(false));

    let refresh_games = {
        let window_weak = window.as_weak();
        let engine = Rc::clone(&engine);
        let system_slugs = Rc::clone(&system_slugs);
        let current_system = Rc::clone(&current_system);
        let current_games = Rc::clone(&current_games);
        move |search: &str| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let slugs = system_slugs.borrow();
            let Some(slug) = slugs.get(current_system.get()) else {
                window.set_games_heading("Games".into());
                window.set_games_model(ModelRc::new(VecModel::from(Vec::<GameItem>::new())));
                return;
            };
            let search = (!search.trim().is_empty()).then_some(search.trim());
            let games = engine
                .borrow()
                .query_games(Some(slug), search)
                .unwrap_or_default();
            window.set_games_heading(format!("Games — {} ({})", slug, games.len()).into());
            let items: Vec<GameItem> = games
                .iter()
                .map(|g| GameItem {
                    id: g.id as i32,
                    name: g.name.clone().into(),
                    favorite: g.favorite,
                })
                .collect();
            window.set_games_model(ModelRc::new(VecModel::from(items)));
            *current_games.borrow_mut() = games;

            let profile = engine
                .borrow()
                .list_launch_profiles()
                .unwrap_or_default()
                .into_iter()
                .filter(|p| &p.system_slug == slug)
                .max_by_key(|p| p.priority);
            let status = match profile {
                Some(p) => format!("Launch profile: {} \"{}\"", p.emulator_name, p.arg_template),
                None => format!("No launch profile for {slug} yet — configure one below."),
            };
            window.set_emulator_status(status.into());
        }
    };

    let refresh_systems = {
        let window_weak = window.as_weak();
        let engine = Rc::clone(&engine);
        let system_slugs = Rc::clone(&system_slugs);
        let current_system = Rc::clone(&current_system);
        let refresh_games = refresh_games.clone();
        move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let systems = engine.borrow().list_systems().unwrap_or_default();
            *system_slugs.borrow_mut() = systems.iter().map(|s| s.slug.clone()).collect();
            if current_system.get() >= systems.len() {
                current_system.set(0);
            }
            let items: Vec<StandardListViewItem> = systems
                .iter()
                .map(|s| {
                    let label = format!("{} ({})", s.name, s.game_count);
                    StandardListViewItem::from(slint::SharedString::from(label))
                })
                .collect();
            window.set_systems_model(ModelRc::new(VecModel::from(items)));
            refresh_games("");
        }
    };

    refresh_systems();

    window.on_system_selected({
        let current_system = Rc::clone(&current_system);
        let refresh_games = refresh_games.clone();
        move |index| {
            if index < 0 {
                return;
            }
            current_system.set(index as usize);
            refresh_games("");
        }
    });

    window.on_search_edited({
        let refresh_games = refresh_games.clone();
        move |text| {
            refresh_games(text.as_str());
        }
    });

    window.on_favorite_toggled({
        let window_weak = window.as_weak();
        let engine = Rc::clone(&engine);
        let current_games = Rc::clone(&current_games);
        let refresh_games = refresh_games.clone();
        move |id| {
            let was_favorite = current_games
                .borrow()
                .iter()
                .find(|g| g.id as i32 == id)
                .map(|g| g.favorite);
            let Some(was_favorite) = was_favorite else {
                return;
            };
            if engine
                .borrow_mut()
                .set_favorite(id as i64, !was_favorite)
                .is_ok()
            {
                let search = window_weak.upgrade().map(|w| w.get_search_text());
                refresh_games(search.as_deref().unwrap_or(""));
            }
        }
    });

    window.on_game_launch_requested({
        let window_weak = window.as_weak();
        let engine = Rc::clone(&engine);
        move |id| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let mut events = Vec::new();
            let result = engine
                .borrow_mut()
                .launch(id as i64, &mut |e| events.push(e));
            let message = match result {
                Ok(_) => {
                    let duration = events.iter().find_map(|e| match e {
                        Event::LaunchEnded { duration_s, .. } => Some(*duration_s),
                        _ => None,
                    });
                    match duration {
                        Some(d) => format!("Played for {d}s"),
                        None => "Launched".to_string(),
                    }
                }
                Err(e) => format!("Launch failed: {e}"),
            };
            window.set_status_line(message.into());
        }
    });

    window.on_browse_emulator_exec({
        let window_weak = window.as_weak();
        move || {
            let Some(path) = rfd::FileDialog::new().pick_file() else {
                return;
            };
            if let Some(window) = window_weak.upgrade() {
                window.set_emulator_exec_input(path.to_string_lossy().into_owned().into());
            }
        }
    });

    window.on_detect_emulators_requested({
        let window_weak = window.as_weak();
        move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let path_var = std::env::var("PATH").unwrap_or_default();
            let found = emulator_detect::detect_emulators(&path_var);
            match found.first() {
                Some(emu) => {
                    window.set_emulator_name_input(emu.name.clone().into());
                    window.set_emulator_exec_input(emu.exec.clone().into());
                    let extra = if found.len() > 1 {
                        format!(
                            " ({} more found — Browse… to pick a different one)",
                            found.len() - 1
                        )
                    } else {
                        String::new()
                    };
                    window.set_status_line(format!("Detected {}{extra}", emu.name).into());
                }
                None => {
                    window.set_status_line(
                        "No known emulators found on PATH — use Browse… to pick one.".into(),
                    );
                }
            }
        }
    });

    window.on_save_launch_profile({
        let window_weak = window.as_weak();
        let engine = Rc::clone(&engine);
        let system_slugs = Rc::clone(&system_slugs);
        let current_system = Rc::clone(&current_system);
        let refresh_games = refresh_games.clone();
        move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let slugs = system_slugs.borrow();
            let Some(slug) = slugs.get(current_system.get()).cloned() else {
                return;
            };
            drop(slugs);
            let name = window.get_emulator_name_input().to_string();
            let exec = window.get_emulator_exec_input().to_string();
            let args = window.get_emulator_args_input().to_string();
            if name.trim().is_empty() || exec.trim().is_empty() {
                window.set_status_line("Emulator name and executable path are required.".into());
                return;
            }
            let mut eng = engine.borrow_mut();
            let result = eng
                .add_emulator(name.trim(), exec.trim())
                .and_then(|_| eng.add_launch_profile(name.trim(), &slug, args.trim(), 0));
            match result {
                Ok(_) => {
                    window.set_status_line(format!("Saved launch profile for {slug}.").into());
                }
                Err(e) => {
                    window.set_status_line(format!("Failed to save profile: {e}").into());
                }
            }
            drop(eng);
            let search = window.get_search_text();
            refresh_games(search.as_str());
        }
    });

    window.on_add_library_requested({
        let window_weak = window.as_weak();
        let engine = Rc::clone(&engine);
        move || {
            let Some(folder) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let name = folder
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| folder.to_string_lossy().into_owned());
            let mut eng = engine.borrow_mut();
            let library_id = match eng.add_library(&folder, &name) {
                Ok(id) => id,
                Err(e) => {
                    window.set_status_line(format!("Failed to add {name}: {e}").into());
                    return;
                }
            };
            if let Err(e) = eng.scan(library_id, &mut |_event| {}) {
                window.set_status_line(format!("Scan of {name} failed: {e}").into());
                return;
            }
            window.set_status_line(format!("core {} — scanned {name}", eng.version()).into());
            drop(eng);
            refresh_systems();
        }
    });

    window.on_stats_toggled({
        let window_weak = window.as_weak();
        let engine = Rc::clone(&engine);
        let show_stats = Rc::clone(&show_stats);
        let show_scraper = Rc::clone(&show_scraper);
        let show_detail = Rc::clone(&show_detail);
        move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let new_value = !show_stats.get();
            show_stats.set(new_value);
            window.set_show_stats(new_value);
            if new_value {
                show_scraper.set(false);
                window.set_show_scraper(false);
                show_detail.set(false);
                window.set_show_detail(false);
            } else {
                return;
            }

            let eng = engine.borrow();

            let to_rows = |stats: Vec<relic_core::stats::GameStats>| -> Vec<StatRow> {
                stats
                    .iter()
                    .map(|g| StatRow {
                        name: g.name.clone().into(),
                        subtitle: format!(
                            "{} — {}x, {}m total, last {}",
                            g.system_slug,
                            g.play_count,
                            g.total_seconds / 60,
                            g.last_played_at
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| "-".into())
                        )
                        .into(),
                    })
                    .collect()
            };

            let recent_rows = eng.recently_played(20).map(to_rows).unwrap_or_default();
            let most_played_rows = eng.most_played(20).map(to_rows).unwrap_or_default();
            let summary = match eng.play_totals() {
                Ok((sessions, total_seconds)) => {
                    format!("{sessions} sessions, {}m total", total_seconds / 60)
                }
                Err(_) => "stats unavailable".to_string(),
            };
            drop(eng);

            window.set_recent_model(ModelRc::new(VecModel::from(recent_rows)));
            window.set_most_played_model(ModelRc::new(VecModel::from(most_played_rows)));
            window.set_stats_summary(summary.into());
        }
    });

    let refresh_pending_matches = {
        let window_weak = window.as_weak();
        let engine = Rc::clone(&engine);
        let scraper_conn = Rc::clone(&scraper_conn);
        move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let conn = scraper_conn.borrow();
            let pending = relic_scraper::pending_matches(&conn).unwrap_or_default();
            drop(conn);
            let games = engine.borrow().query_games(None, None).unwrap_or_default();
            let rows: Vec<PendingMatchRow> = pending
                .iter()
                .map(|p| {
                    let name = games
                        .iter()
                        .find(|g| g.id == p.game_id)
                        .map(|g| g.name.clone())
                        .unwrap_or_else(|| format!("game #{}", p.game_id));
                    PendingMatchRow {
                        game_id: p.game_id as i32,
                        name: name.into(),
                        provider_id: p.provider_id.clone().into(),
                        confidence: p.confidence.as_str().into(),
                        external_id: p.external_id.clone().into(),
                    }
                })
                .collect();
            window.set_pending_matches_model(ModelRc::new(VecModel::from(rows)));
        }
    };

    window.on_scraper_toggled({
        let window_weak = window.as_weak();
        let show_stats = Rc::clone(&show_stats);
        let show_scraper = Rc::clone(&show_scraper);
        let show_detail = Rc::clone(&show_detail);
        let refresh_pending_matches = refresh_pending_matches.clone();
        move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let new_value = !show_scraper.get();
            show_scraper.set(new_value);
            window.set_show_scraper(new_value);
            if new_value {
                show_stats.set(false);
                window.set_show_stats(false);
                show_detail.set(false);
                window.set_show_detail(false);
                refresh_pending_matches();
            }
        }
    });

    window.on_match_confirmed({
        let scraper_conn = Rc::clone(&scraper_conn);
        let refresh_pending_matches = refresh_pending_matches.clone();
        move |game_id, provider_id| {
            let conn = scraper_conn.borrow();
            let _ = relic_scraper::confirm_match(&conn, game_id as i64, provider_id.as_str());
            drop(conn);
            refresh_pending_matches();
        }
    });

    let show_detail_for = |window: &MainWindow, game: &GameRow| {
        window.set_detail_game_id(game.id as i32);
        window.set_detail_name(game.name.clone().into());
        window.set_detail_system(game.system_slug.clone().into());
        window.set_detail_path(game.rel_path.clone().unwrap_or_default().into());
        window.set_detail_favorite(game.favorite);
    };

    window.on_game_info_requested({
        let window_weak = window.as_weak();
        let current_games = Rc::clone(&current_games);
        let show_stats = Rc::clone(&show_stats);
        let show_scraper = Rc::clone(&show_scraper);
        let show_detail = Rc::clone(&show_detail);
        move |id| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let games = current_games.borrow();
            let Some(game) = games.iter().find(|g| g.id as i32 == id) else {
                return;
            };
            show_detail_for(&window, game);
            drop(games);
            show_stats.set(false);
            window.set_show_stats(false);
            show_scraper.set(false);
            window.set_show_scraper(false);
            show_detail.set(true);
            window.set_show_detail(true);
        }
    });

    window.on_detail_play_requested({
        let window_weak = window.as_weak();
        let engine = Rc::clone(&engine);
        move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let id = window.get_detail_game_id();
            let mut events = Vec::new();
            let result = engine
                .borrow_mut()
                .launch(id as i64, &mut |e| events.push(e));
            let message = match result {
                Ok(_) => {
                    let duration = events.iter().find_map(|e| match e {
                        Event::LaunchEnded { duration_s, .. } => Some(*duration_s),
                        _ => None,
                    });
                    match duration {
                        Some(d) => format!("Played for {d}s"),
                        None => "Launched".to_string(),
                    }
                }
                Err(e) => format!("Launch failed: {e}"),
            };
            window.set_status_line(message.into());
        }
    });

    window.on_detail_favorite_toggled({
        let window_weak = window.as_weak();
        let engine = Rc::clone(&engine);
        let refresh_games = refresh_games.clone();
        move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let id = window.get_detail_game_id();
            let new_favorite = !window.get_detail_favorite();
            if engine
                .borrow_mut()
                .set_favorite(id as i64, new_favorite)
                .is_ok()
            {
                window.set_detail_favorite(new_favorite);
                let search = window.get_search_text();
                refresh_games(search.as_str());
            }
        }
    });

    window.on_detail_back_requested({
        let window_weak = window.as_weak();
        let show_detail = Rc::clone(&show_detail);
        move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            show_detail.set(false);
            window.set_show_detail(false);
        }
    });

    window.run()
}
