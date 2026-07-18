//! relic-cli — headless CLI over relic-core (PLAN.md §5 "Platform integration").
//!
//! Doubles as the project's test harness: everything the shells can do
//! should be reachable and scriptable from here first.

use std::error::Error;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use relic_core::api::Engine;
use relic_core::events::Event;

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
    let summary = engine.scan(library_id, &mut |event| {
        if let Event::ScanProgress { done, total, .. } = event {
            eprint!("\r{done}/{total}");
        }
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
        println!("[{star}] {} ({})", g.name, g.system_slug);
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
