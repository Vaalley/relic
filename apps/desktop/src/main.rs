//! Relic desktop shell (Phase 2, early).
//!
//! UI stack decision (ADR-002, `docs/adr/0002-desktop-ui-stack.md`) is settled:
//! Slint. This is a system/game browser skeleton — real `Engine` data, no
//! detail page, launch, gamepad input, or theming yet (PLAN.md Phase 2 exit
//! criteria are still ahead).
//!
//! The library database is a real file under the OS data dir (persists
//! across launches — added libraries stay added). In debug builds, if that
//! database has no games yet, `fixtures/mini` is scanned in as a one-time
//! seed so the views aren't empty before a real folder is added.

slint::include_modules!();

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
    window.set_status_line(format!("core {} — {}", engine.version(), db_path.display()).into());

    let engine = Rc::new(RefCell::new(engine));
    let system_slugs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let current_system: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    let current_games: Rc<RefCell<Vec<GameRow>>> = Rc::new(RefCell::new(Vec::new()));

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

    window.run()
}
