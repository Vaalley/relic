//! relic-cli — headless CLI over relic-core (PLAN.md §5 "Platform integration").
//!
//! Doubles as the project's test harness: everything the shells can do
//! should be reachable and scriptable from here first.

use std::error::Error;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use relic_core::api::Engine;
use relic_core::collections::SmartQuery;
use relic_core::events::Event;
use relic_core::intents;
use relic_core::stats::GameStats;
use relic_core::systems::builtin_systems;

#[derive(Parser)]
#[command(name = "relic", about = "Relic headless CLI: scan, query, launch")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List registered systems and their game counts.
    Systems {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
    },
    /// Add (or reuse) a library at ROOT and (re)scan it.
    Scan {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        root: PathBuf,
    },
    /// List games, optionally filtered by system slug and/or search text.
    Games {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        #[arg(long)]
        system: Option<String>,
        #[arg(long)]
        search: Option<String>,
    },
    /// Run the database integrity check and report engine/schema health.
    Doctor {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
    },
    /// Import <root>/<system>/gamelist.xml metadata for a scanned library.
    ImportGamelists {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        root: PathBuf,
    },
    /// Export <root>/<system>/gamelist.xml for every system with games in
    /// this library (interop back out to other frontends).
    ExportGamelists {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        root: PathBuf,
    },
    /// Discover local artwork and refresh the thumbnail cache.
    RefreshMedia {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        root: PathBuf,
    },
    /// Show indexed media for a game id.
    Media {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        game_id: i64,
    },
    /// Compute missing CRC32/MD5 hashes for indexed files.
    Hash {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        /// Maximum files to hash this run (0 = everything pending).
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Register an emulator executable for this platform.
    EmulatorAdd {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        /// Short unique name, e.g. "retroarch".
        name: String,
        /// Path to (or PATH-resolvable name of) the executable.
        exec: String,
    },
    /// List registered emulators.
    Emulators {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
    },
    /// Attach a launch profile: which emulator + arguments a system uses.
    ProfileAdd {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        /// Emulator name as registered with emulator-add.
        emulator: String,
        /// System slug, e.g. "snes".
        system: String,
        /// Argument template; placeholders: {rom} {rom_dir} {core}.
        template: String,
        #[arg(long, default_value_t = 0)]
        priority: i64,
    },
    /// List launch profiles.
    Profiles {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
    },
    /// Launch a game by id (shown by `games`) and wait for the emulator.
    Launch {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        game_id: i64,
        /// Print the resolved command line instead of running it.
        #[arg(long)]
        dry_run: bool,
    },
    /// List the most recently played games.
    Recent {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// List games ordered by total playtime.
    MostPlayed {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Show library-wide play totals (sessions and time played).
    Stats {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
    },
    /// Create an empty manual collection.
    CollectionAdd {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        name: String,
    },
    /// Create a smart collection (membership computed live from filters).
    CollectionAddSmart {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        name: String,
        #[arg(long)]
        system: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        favorite: Option<bool>,
    },
    /// List collections.
    Collections {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
    },
    /// List a collection's member games.
    CollectionGames {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        collection_id: i64,
    },
    /// Add a game to a manual collection.
    CollectionAddGame {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        collection_id: i64,
        game_id: i64,
    },
    /// Remove a game from a manual collection.
    CollectionRemoveGame {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        collection_id: i64,
        game_id: i64,
    },
    /// Delete a collection (manual or smart).
    CollectionDelete {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        collection_id: i64,
    },
    /// Match a No-Intro/Redump DAT file's entries against already-hashed
    /// files for one system, updating canonical names on each CRC32 hit.
    DatImport {
        #[arg(long, default_value = "relic.db")]
        db: PathBuf,
        system: String,
        dat_path: PathBuf,
    },
    /// List the built-in Android intent templates (core/data/intents/).
    Intents,
    /// Validate Android intent template(s) against docs/android-intents.md §6.
    IntentValidate {
        /// Validate this TOML file instead of the built-in set.
        path: Option<PathBuf>,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Systems { db } => cmd_systems(&db),
        Command::Scan { db, root } => cmd_scan(&db, &root),
        Command::Games { db, system, search } => {
            cmd_games(&db, system.as_deref(), search.as_deref())
        }
        Command::Doctor { db } => cmd_doctor(&db),
        Command::ImportGamelists { db, root } => cmd_import_gamelists(&db, &root),
        Command::ExportGamelists { db, root } => cmd_export_gamelists(&db, &root),
        Command::RefreshMedia { db, root } => cmd_refresh_media(&db, &root),
        Command::Media { db, game_id } => cmd_media(&db, game_id),
        Command::Hash { db, limit } => cmd_hash(&db, limit),
        Command::EmulatorAdd { db, name, exec } => cmd_emulator_add(&db, &name, &exec),
        Command::Emulators { db } => cmd_emulators(&db),
        Command::ProfileAdd {
            db,
            emulator,
            system,
            template,
            priority,
        } => cmd_profile_add(&db, &emulator, &system, &template, priority),
        Command::Profiles { db } => cmd_profiles(&db),
        Command::Launch {
            db,
            game_id,
            dry_run,
        } => cmd_launch(&db, game_id, dry_run),
        Command::Recent { db, limit } => cmd_recent(&db, limit),
        Command::MostPlayed { db, limit } => cmd_most_played(&db, limit),
        Command::Stats { db } => cmd_stats(&db),
        Command::CollectionAdd { db, name } => cmd_collection_add(&db, &name),
        Command::CollectionAddSmart {
            db,
            name,
            system,
            search,
            favorite,
        } => cmd_collection_add_smart(&db, &name, system, search, favorite),
        Command::Collections { db } => cmd_collections(&db),
        Command::CollectionGames { db, collection_id } => cmd_collection_games(&db, collection_id),
        Command::CollectionAddGame {
            db,
            collection_id,
            game_id,
        } => cmd_collection_add_game(&db, collection_id, game_id),
        Command::CollectionRemoveGame {
            db,
            collection_id,
            game_id,
        } => cmd_collection_remove_game(&db, collection_id, game_id),
        Command::CollectionDelete { db, collection_id } => {
            cmd_collection_delete(&db, collection_id)
        }
        Command::DatImport {
            db,
            system,
            dat_path,
        } => cmd_dat_import(&db, &system, &dat_path),
        Command::Intents => cmd_intents(),
        Command::IntentValidate { path } => cmd_intent_validate(path.as_deref()),
    }
}

fn cmd_systems(db: &Path) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    let systems = engine.list_systems()?;
    println!("{:<20} {:<30} {:>10}", "SLUG", "NAME", "GAMES");
    for s in systems {
        println!("{:<20} {:<30} {:>10}", s.slug, s.name, s.game_count);
    }
    Ok(())
}

fn cmd_scan(db: &Path, root: &Path) -> Result<(), Box<dyn Error>> {
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("library")
        .to_string();
    let mut engine = Engine::open(db)?;
    let library_id = engine.add_library(root, &name)?;
    let summary = engine.scan(library_id, &mut |event| match event {
        Event::ScanProgress { done, total, .. } => eprint!("\r{done}/{total}"),
        Event::Warning { code, context } => eprintln!("warning [{code}]: {context}"),
        _ => {}
    })?;
    eprintln!();
    println!(
        "added={} removed={} unchanged={}",
        summary.added, summary.removed, summary.unchanged
    );
    Ok(())
}

fn cmd_games(db: &Path, system: Option<&str>, search: Option<&str>) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    let games = engine.query_games(system, search)?;
    for g in games {
        let star = if g.favorite { '*' } else { ' ' };
        println!("[{star}] #{:<5} {} ({})", g.id, g.name, g.system_slug);
    }
    Ok(())
}

fn cmd_import_gamelists(db: &Path, root: &Path) -> Result<(), Box<dyn Error>> {
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("library")
        .to_string();
    let mut engine = Engine::open(db)?;
    let library_id = engine.add_library(root, &name)?;
    let stats = engine.import_gamelists(library_id, &mut |event| {
        if let Event::Warning { code, context } = event {
            eprintln!("warning [{code}]: {context}");
        }
    })?;
    println!("matched={} unmatched={}", stats.matched, stats.unmatched);
    Ok(())
}

fn cmd_export_gamelists(db: &Path, root: &Path) -> Result<(), Box<dyn Error>> {
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("library")
        .to_string();
    let mut engine = Engine::open(db)?;
    let library_id = engine.add_library(root, &name)?;
    let written = engine.export_gamelists(library_id)?;
    println!("wrote {written} gamelist.xml file(s)");
    Ok(())
}

fn cmd_collection_add(db: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::open(db)?;
    let id = engine.create_manual_collection(name)?;
    println!("created manual collection #{id} '{name}'");
    Ok(())
}

fn cmd_collection_add_smart(
    db: &Path,
    name: &str,
    system: Option<String>,
    search: Option<String>,
    favorite: Option<bool>,
) -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::open(db)?;
    let query = SmartQuery {
        system,
        search,
        favorite,
    };
    let id = engine.create_smart_collection(name, &query)?;
    println!("created smart collection #{id} '{name}'");
    Ok(())
}

fn cmd_collections(db: &Path) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    for c in engine.list_collections()? {
        println!("#{:<5} {:<8} {}", c.id, c.kind, c.name);
    }
    Ok(())
}

fn cmd_collection_games(db: &Path, collection_id: i64) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    for g in engine.collection_games(collection_id)? {
        let star = if g.favorite { '*' } else { ' ' };
        println!("[{star}] #{:<5} {} ({})", g.id, g.name, g.system_slug);
    }
    Ok(())
}

fn cmd_collection_add_game(
    db: &Path,
    collection_id: i64,
    game_id: i64,
) -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::open(db)?;
    engine.add_to_collection(collection_id, game_id)?;
    println!("added game #{game_id} to collection #{collection_id}");
    Ok(())
}

fn cmd_collection_remove_game(
    db: &Path,
    collection_id: i64,
    game_id: i64,
) -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::open(db)?;
    engine.remove_from_collection(collection_id, game_id)?;
    println!("removed game #{game_id} from collection #{collection_id}");
    Ok(())
}

fn cmd_collection_delete(db: &Path, collection_id: i64) -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::open(db)?;
    engine.delete_collection(collection_id)?;
    println!("deleted collection #{collection_id}");
    Ok(())
}

fn cmd_dat_import(db: &Path, system: &str, dat_path: &Path) -> Result<(), Box<dyn Error>> {
    let xml = std::fs::read_to_string(dat_path)?;
    let mut engine = Engine::open(db)?;
    let stats = engine.import_dat(system, &xml)?;
    println!("matched={} unmatched={}", stats.matched, stats.unmatched);
    Ok(())
}

fn cmd_refresh_media(db: &Path, root: &Path) -> Result<(), Box<dyn Error>> {
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("library")
        .to_string();
    let mut engine = Engine::open(db)?;
    let library_id = engine.add_library(root, &name)?;
    let stats = engine.refresh_media(library_id, &mut |event| {
        if let Event::Warning { code, context } = event {
            eprintln!("warning [{code}]: {context}");
        }
    })?;
    println!(
        "discovered={} thumbnails_cached={} failed={}",
        stats.discovered, stats.thumbnails_cached, stats.failed
    );
    Ok(())
}

fn cmd_media(db: &Path, game_id: i64) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    for m in engine.game_media(game_id)? {
        let thumb = if m.cache_hash.is_empty() {
            "(from source)".to_string()
        } else {
            engine
                .thumbnail_path(&m.cache_hash)
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        };
        println!(
            "{:<12} {:<14} {} {}",
            m.kind, m.source, m.source_path, thumb
        );
    }
    Ok(())
}

fn cmd_hash(db: &Path, limit: usize) -> Result<(), Box<dyn Error>> {
    let limit = if limit == 0 { usize::MAX } else { limit };
    let mut engine = Engine::open(db)?;
    let stats = engine.hash_pending(None, limit, &mut |event| {
        if let Event::Warning { code, context } = event {
            eprintln!("warning [{code}]: {context}");
        }
    })?;
    println!(
        "hashed={} skipped={} failed={}",
        stats.hashed, stats.skipped, stats.failed
    );
    Ok(())
}

fn cmd_emulator_add(db: &Path, name: &str, exec: &str) -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::open(db)?;
    let id = engine.add_emulator(name, exec)?;
    println!("registered emulator #{id} '{name}' -> {exec}");
    Ok(())
}

fn cmd_emulators(db: &Path) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    println!("{:<6} {:<16} {:<8} EXEC", "ID", "NAME", "OS");
    for e in engine.list_emulators()? {
        println!("{:<6} {:<16} {:<8} {}", e.id, e.name, e.platform, e.exec);
    }
    Ok(())
}

fn cmd_profile_add(
    db: &Path,
    emulator: &str,
    system: &str,
    template: &str,
    priority: i64,
) -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::open(db)?;
    let id = engine.add_launch_profile(emulator, system, template, priority)?;
    println!("added profile #{id}: {system} -> {emulator} `{template}`");
    Ok(())
}

fn cmd_profiles(db: &Path) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    println!(
        "{:<6} {:<12} {:<16} {:<4} TEMPLATE",
        "ID", "SYSTEM", "EMULATOR", "PRI"
    );
    for p in engine.list_launch_profiles()? {
        println!(
            "{:<6} {:<12} {:<16} {:<4} {}",
            p.id, p.system_slug, p.emulator_name, p.priority, p.arg_template
        );
    }
    Ok(())
}

fn cmd_launch(db: &Path, game_id: i64, dry_run: bool) -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::open(db)?;
    let plan = engine.resolve_launch(game_id)?;
    if dry_run {
        println!("{} {}", plan.exec, plan.args.join(" "));
        return Ok(());
    }
    eprintln!("launching {} …", plan.rom_path.display());
    let session = engine.launch(game_id, &mut |event| {
        if let Event::LaunchEnded { duration_s, .. } = event {
            eprintln!("session ended after {duration_s}s");
        }
    })?;
    println!("recorded play session #{session}");
    Ok(())
}

fn cmd_recent(db: &Path, limit: usize) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    for g in engine.recently_played(limit)? {
        print_game_stats(&g);
    }
    Ok(())
}

fn cmd_most_played(db: &Path, limit: usize) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    for g in engine.most_played(limit)? {
        print_game_stats(&g);
    }
    Ok(())
}

fn print_game_stats(g: &GameStats) {
    let minutes = g.total_seconds / 60;
    let last = g
        .last_played_at
        .map(|t| t.to_string())
        .unwrap_or_else(|| "-".to_string());
    println!(
        "{} ({}) — {}x, {}m total, last {}",
        g.name, g.system_slug, g.play_count, minutes, last
    );
}

fn cmd_stats(db: &Path) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    let (sessions, total_time) = engine.play_totals()?;
    println!("sessions={sessions} total_time={total_time}s");
    Ok(())
}

fn cmd_intents() -> Result<(), Box<dyn Error>> {
    println!(
        "{:<18} {:<24} {:<10} PACKAGE",
        "ID", "DISPLAY NAME", "DATA MODE"
    );
    for (stem, t) in intents::builtin_intents()? {
        println!(
            "{:<18} {:<24} {:<10} {}",
            stem,
            t.display_name,
            format!("{:?}", t.data_mode).to_lowercase(),
            t.package
        );
    }
    Ok(())
}

fn cmd_intent_validate(path: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let slugs: Vec<String> = builtin_systems()?.into_iter().map(|s| s.slug).collect();
    let mut any_failed = false;

    let targets: Vec<(String, String)> = match path {
        Some(p) => {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let src = std::fs::read_to_string(p)?;
            vec![(stem, src)]
        }
        None => intents::builtin_sources()
            .iter()
            .map(|(stem, src)| (stem.to_string(), src.to_string()))
            .collect(),
    };

    for (stem, src) in targets {
        match intents::parse_intent(&src) {
            Ok(template) => {
                let errors = intents::validate(&template, &stem, &slugs);
                if errors.is_empty() {
                    println!("OK    {stem}");
                } else {
                    any_failed = true;
                    println!("FAIL  {stem}");
                    for e in errors {
                        println!("        - {e}");
                    }
                }
            }
            Err(e) => {
                any_failed = true;
                println!("FAIL  {stem}");
                println!("        - {e}");
            }
        }
    }

    if any_failed {
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_doctor(db: &Path) -> Result<(), Box<dyn Error>> {
    let engine = Engine::open(db)?;
    println!("relic-core version: {}", engine.version());
    let ok = engine.integrity_check()?;
    if ok {
        println!("OK");
        Ok(())
    } else {
        println!("CORRUPT");
        std::process::exit(1);
    }
}
