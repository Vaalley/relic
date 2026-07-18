//! Emulator profiles and launch model (Phase 1, PLAN.md §4.5).
//!
//! Core owns profile storage, resolution (game → emulator + expanded argv),
//! and play-session bookkeeping. The desktop path spawns the child process
//! directly ([`run_blocking`]); the Android shell will consume [`LaunchPlan`]
//! and fire an Intent instead (templates in `core/data/intents/`, Phase 3).
//! Still to come per the plan: {rom_extracted} temp-extraction for emulators
//! that can't read archives, pre/post hooks, emulator auto-detection.

pub mod template;

use std::path::PathBuf;
use std::process::Command;

use crate::db::Db;
use crate::events::Event;
use crate::systems::SystemDef;
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct EmulatorRow {
    pub id: i64,
    pub name: String,
    pub platform: String,
    pub exec: String,
}

#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub id: i64,
    pub emulator_name: String,
    pub system_slug: String,
    pub arg_template: String,
    pub priority: i64,
}

/// A fully resolved launch: what to execute and with which arguments.
#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub game_id: i64,
    pub exec: String,
    pub args: Vec<String>,
    pub rom_path: PathBuf,
}

/// Platform key used to filter emulator rows; matches `emulators.platform`.
pub fn current_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

pub(crate) fn add_emulator(db: &Db, name: &str, platform: &str, exec: &str) -> Result<i64> {
    db.conn().execute(
        "INSERT INTO emulators (name, platform, exec_or_package) VALUES (?1, ?2, ?3)",
        rusqlite::params![name, platform, exec],
    )?;
    Ok(db.conn().last_insert_rowid())
}

pub(crate) fn emulator_id_by_name(db: &Db, name: &str) -> Result<i64> {
    db.conn()
        .query_row("SELECT id FROM emulators WHERE name=?1", [name], |r| {
            r.get(0)
        })
        .map_err(|_| Error::EmulatorNotFound(name.to_string()))
}

pub(crate) fn list_emulators(db: &Db) -> Result<Vec<EmulatorRow>> {
    let mut stmt = db
        .conn()
        .prepare("SELECT id, name, platform, exec_or_package FROM emulators ORDER BY name")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(EmulatorRow {
                id: r.get(0)?,
                name: r.get(1)?,
                platform: r.get(2)?,
                exec: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub(crate) fn add_profile(
    db: &Db,
    emulator_id: i64,
    system_id: i64,
    arg_template: &str,
    priority: i64,
) -> Result<i64> {
    // Validate the template shape now so a typo fails at configuration time,
    // not at launch. Placeholder names are checked at resolve time instead.
    template::expand(arg_template, PLACEHOLDER_PROBE)?;
    db.conn().execute(
        "INSERT INTO launch_profiles (emulator_id, system_id, arg_template, priority)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![emulator_id, system_id, arg_template, priority],
    )?;
    Ok(db.conn().last_insert_rowid())
}

/// Every placeholder the template language defines, with dummy values —
/// used to syntax-check templates independent of a concrete game.
const PLACEHOLDER_PROBE: &[(&str, &str)] = &[("rom", "x"), ("rom_dir", "x"), ("core", "x")];

pub(crate) fn list_profiles(db: &Db) -> Result<Vec<ProfileRow>> {
    let mut stmt = db.conn().prepare(
        "SELECT lp.id, e.name, s.slug, lp.arg_template, lp.priority
         FROM launch_profiles lp
         JOIN emulators e ON e.id = lp.emulator_id
         JOIN systems s ON s.id = lp.system_id
         ORDER BY s.slug, lp.priority DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ProfileRow {
                id: r.get(0)?,
                emulator_name: r.get(1)?,
                system_slug: r.get(2)?,
                arg_template: r.get(3)?,
                priority: r.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Resolve a game to a concrete exec + argv using the highest-priority
/// profile for its system on the current platform.
pub(crate) fn resolve(db: &Db, game_id: i64, systems: &[SystemDef]) -> Result<LaunchPlan> {
    let (system_id, slug): (i64, String) = db
        .conn()
        .query_row(
            "SELECT g.system_id, s.slug FROM games g JOIN systems s ON s.id = g.system_id
             WHERE g.id = ?1",
            [game_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| Error::GameNotFound(game_id))?;

    let (rel_path, root): (String, String) = db
        .conn()
        .query_row(
            "SELECT f.rel_path, l.root_uri FROM files f JOIN libraries l ON l.id = f.library_id
             WHERE f.game_id = ?1 LIMIT 1",
            [game_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| Error::GameNotFound(game_id))?;

    let (exec, arg_template): (String, String) = db
        .conn()
        .query_row(
            "SELECT e.exec_or_package, lp.arg_template
             FROM launch_profiles lp JOIN emulators e ON e.id = lp.emulator_id
             WHERE lp.system_id = ?1 AND e.platform = ?2
             ORDER BY lp.priority DESC LIMIT 1",
            rusqlite::params![system_id, current_platform()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| Error::NoLaunchProfile(slug.clone()))?;

    let rom_path = PathBuf::from(&root).join(&rel_path);
    let rom = rom_path.to_string_lossy().into_owned();
    let rom_dir = rom_path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let core = systems
        .iter()
        .find(|s| s.slug == slug)
        .and_then(|s| s.default_core.clone())
        .unwrap_or_default();

    let vars: Vec<(&str, &str)> = vec![("rom", &rom), ("rom_dir", &rom_dir), ("core", &core)];
    let args = template::expand(&arg_template, &vars)?;
    Ok(LaunchPlan {
        game_id,
        exec,
        args,
        rom_path,
    })
}

/// Spawn the plan's process, record the play session, and block until the
/// emulator exits. Desktop shells call this from a worker thread and shed
/// their render surface while it runs (PLAN.md §4.5); the CLI just blocks.
pub(crate) fn run_blocking(db: &Db, plan: &LaunchPlan, sink: &mut dyn FnMut(Event)) -> Result<i64> {
    db.conn().execute(
        "INSERT INTO play_sessions (game_id, started_at) VALUES (?1, unixepoch())",
        [plan.game_id],
    )?;
    let session_id = db.conn().last_insert_rowid();
    sink(Event::LaunchStarted {
        game_id: plan.game_id,
        session_id,
    });

    let status = Command::new(&plan.exec)
        .args(&plan.args)
        .spawn()
        .and_then(|mut child| child.wait());

    db.conn().execute(
        "UPDATE play_sessions
         SET ended_at = unixepoch(), duration_s = unixepoch() - started_at
         WHERE id = ?1",
        [session_id],
    )?;
    let duration_s: i64 = db.conn().query_row(
        "SELECT duration_s FROM play_sessions WHERE id = ?1",
        [session_id],
        |r| r.get(0),
    )?;
    sink(Event::LaunchEnded {
        game_id: plan.game_id,
        session_id,
        duration_s,
    });

    match status {
        Ok(exit) if exit.success() => Ok(session_id),
        Ok(exit) => {
            sink(Event::Warning {
                code: "launch.nonzero_exit".into(),
                context: format!("{} exited with {exit}", plan.exec),
            });
            Ok(session_id)
        }
        Err(e) => Err(Error::LaunchFailed {
            exec: plan.exec.clone(),
            reason: e.to_string(),
        }),
    }
}
