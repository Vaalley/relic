//! Filesystem crawl → index pipeline (PLAN.md §4.2).
//!
//! Phase-1 starting point: synchronous, incremental by (size, mtime) quick
//! key, one file == one game, systems resolved by the ES-style convention of
//! per-system subfolders (`<root>/<slug>/...`). Still to come per the plan:
//! background thread pool, archive enumeration, multi-disc grouping, lazy
//! hashing, FS watching.

use std::collections::HashSet;
use std::path::Path;
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

use crate::db::Db;
use crate::events::Event;
use crate::systems::SystemDef;
use crate::Result;

pub struct ScanSummary {
    pub added: u64,
    pub removed: u64,
    pub unchanged: u64,
}

/// Scan one library root. Emits progress events through `sink`.
pub fn scan_library(
    db: &mut Db,
    library_id: i64,
    root: &Path,
    systems: &[SystemDef],
    sink: &mut dyn FnMut(Event),
) -> Result<ScanSummary> {
    sink(Event::ScanStarted { library_id });

    // Collect candidate files first so progress has a stable total.
    let mut candidates: Vec<(i64, std::path::PathBuf)> = Vec::new(); // (system idx as i64, abs path)
    for (idx, sys) in systems.iter().enumerate() {
        let sys_root = root.join(&sys.slug);
        if !sys_root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&sys_root).follow_links(false) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    sink(Event::Warning {
                        code: "scan.unreadable".into(),
                        context: e.to_string(),
                    });
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_default();
            if sys.extensions.contains(&ext) {
                candidates.push((idx as i64, entry.into_path()));
            }
        }
    }

    let total = candidates.len() as u64;
    let mut summary = ScanSummary {
        added: 0,
        removed: 0,
        unchanged: 0,
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut changed_systems: HashSet<i64> = HashSet::new();

    let tx = db.conn_mut().transaction()?;
    for (done, (sys_idx, path)) in candidates.iter().enumerate() {
        let sys = &systems[*sys_idx as usize];
        let meta = path.metadata().map_err(|e| crate::Error::Io {
            path: path.clone(),
            source: e,
        })?;
        let size = meta.len() as i64;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let quick_key = format!("{size}:{mtime}");
        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        seen.insert(rel_path.clone());

        let existing: Option<String> = tx
            .query_row(
                "SELECT quick_key FROM files WHERE library_id=?1 AND rel_path=?2 AND in_archive IS NULL",
                rusqlite::params![library_id, rel_path],
                |r| r.get(0),
            )
            .ok();

        match existing {
            Some(k) if k == quick_key => summary.unchanged += 1,
            Some(_) => {
                tx.execute(
                    "UPDATE files SET size=?1, mtime=?2, quick_key=?3, crc32=NULL, md5=NULL
                     WHERE library_id=?4 AND rel_path=?5 AND in_archive IS NULL",
                    rusqlite::params![size, mtime, quick_key, library_id, rel_path],
                )?;
                let system_id = system_db_id(&tx, &sys.slug)?;
                changed_systems.insert(system_id);
            }
            None => {
                let system_id = system_db_id(&tx, &sys.slug)?;
                let name = display_name(path);
                tx.execute(
                    "INSERT INTO games (system_id, canonical_name, sort_name) VALUES (?1, ?2, ?3)",
                    rusqlite::params![system_id, name, sort_key(&name)],
                )?;
                let game_id = tx.last_insert_rowid();
                tx.execute(
                    "INSERT INTO files (game_id, library_id, rel_path, size, mtime, quick_key)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![game_id, library_id, rel_path, size, mtime, quick_key],
                )?;
                summary.added += 1;
                changed_systems.insert(system_id);
            }
        }

        if done % 200 == 0 || done as u64 + 1 == total {
            sink(Event::ScanProgress {
                library_id,
                done: done as u64 + 1,
                total,
            });
        }
    }

    // Remove index rows whose files vanished from disk (games with no files go too).
    {
        let mut stmt = tx.prepare(
            "SELECT id, rel_path, game_id FROM files WHERE library_id=?1 AND in_archive IS NULL",
        )?;
        let stale: Vec<(i64, i64)> = stmt
            .query_map([library_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .filter_map(|row| row.ok())
            .filter(|(_, rel, _)| !seen.contains(rel))
            .map(|(id, _, game_id)| (id, game_id))
            .collect();
        drop(stmt);
        for (file_id, game_id) in stale {
            tx.execute("DELETE FROM files WHERE id=?1", [file_id])?;
            let remaining: i64 = tx.query_row(
                "SELECT COUNT(*) FROM files WHERE game_id=?1",
                [game_id],
                |r| r.get(0),
            )?;
            if remaining == 0 {
                let system_id: i64 =
                    tx.query_row("SELECT system_id FROM games WHERE id=?1", [game_id], |r| {
                        r.get(0)
                    })?;
                tx.execute("DELETE FROM games WHERE id=?1", [game_id])?;
                changed_systems.insert(system_id);
            }
            summary.removed += 1;
        }
    }
    tx.commit()?;

    for system_id in changed_systems {
        sink(Event::GamesChanged { system_id });
    }
    sink(Event::ScanFinished {
        library_id,
        added: summary.added,
        removed: summary.removed,
        unchanged: summary.unchanged,
    });
    Ok(summary)
}

fn system_db_id(conn: &rusqlite::Connection, slug: &str) -> Result<i64> {
    conn.query_row("SELECT id FROM systems WHERE slug=?1", [slug], |r| r.get(0))
        .map_err(|_| crate::Error::UnknownSystem(slug.into()))
}

/// "Super Mario World (USA).sfc" → "Super Mario World (USA)"
fn display_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Naive sort key; DAT-based canonicalization replaces this in Phase 4.
fn sort_key(name: &str) -> String {
    let lower = name.to_lowercase();
    for prefix in ["the ", "a ", "an "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    lower
}
