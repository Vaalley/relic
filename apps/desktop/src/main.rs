//! Relic desktop shell (Phase 2, early).
//!
//! UI stack decision (ADR-002, `docs/adr/0002-desktop-ui-stack.md`) is settled:
//! Slint. This is a system/game browser skeleton — real `Engine` data, no
//! detail page, search, favorites toggle, launch, first-run wizard, gamepad
//! input, or theming yet (PLAN.md Phase 2 exit criteria are still ahead).
//!
//! Demo data: scans `fixtures/mini` on startup so the list views have real
//! rows to show. First-run folder picking (real user libraries) replaces this
//! before Phase 2 is done.

slint::include_modules!();

use std::path::PathBuf;
use std::rc::Rc;

use relic_core::api::Engine;
use slint::{ModelRc, StandardListViewItem, VecModel};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mini")
}

fn main() -> Result<(), slint::PlatformError> {
    let mut engine = Engine::open_in_memory().expect("in-memory engine should always open");

    let root = fixtures_root();
    if root.is_dir() {
        let library_id = engine
            .add_library(&root, "fixtures-mini")
            .expect("adding demo library should not fail");
        engine
            .scan(library_id, &mut |_event| {})
            .expect("scanning demo library should not fail");
    }

    let systems = engine.list_systems().unwrap_or_default();
    let system_slugs: Rc<Vec<String>> = Rc::new(systems.iter().map(|s| s.slug.clone()).collect());

    let systems_items: Vec<StandardListViewItem> = systems
        .iter()
        .map(|s| {
            let label = format!("{} ({})", s.name, s.game_count);
            StandardListViewItem::from(slint::SharedString::from(label))
        })
        .collect();
    let systems_model = ModelRc::new(VecModel::from(systems_items));

    let window = MainWindow::new()?;
    window.set_status_line(
        format!(
            "core {} — {} systems registered",
            engine.version(),
            systems.len()
        )
        .into(),
    );
    window.set_systems_model(systems_model);

    let load_games = {
        let window_weak = window.as_weak();
        let system_slugs = Rc::clone(&system_slugs);
        move |index: usize| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let Some(slug) = system_slugs.get(index) else {
                return;
            };
            let games = engine.query_games(Some(slug), None).unwrap_or_default();
            window.set_games_heading(format!("Games — {} ({})", slug, games.len()).into());
            let items: Vec<GameItem> = games
                .iter()
                .map(|g| GameItem {
                    name: g.name.clone().into(),
                    favorite: g.favorite,
                })
                .collect();
            window.set_games_model(ModelRc::new(VecModel::from(items)));
        }
    };

    if !system_slugs.is_empty() {
        load_games(0);
    } else {
        window.set_games_heading("Games".into());
    }

    window.on_system_selected(move |index| {
        if index >= 0 {
            load_games(index as usize);
        }
    });

    window.run()
}
