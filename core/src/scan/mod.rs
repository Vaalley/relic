//! Filesystem crawl → index pipeline (PLAN.md §4.2).
//!
//! Phase-1 starting point: synchronous, incremental by (size, mtime) quick
//! key, systems resolved by the ES-style convention of per-system subfolders
//! (`<root>/<slug>/...`, folder names matched case-insensitively). Archive-aware: a `.zip` whose system has real ROM
//! extensions (see `archive::should_enumerate`) is enumerated without
//! extraction and can expand into one game per inner entry (`archive.rs`).
//! Multi-disc sets collapse via `.m3u`: a playlist's referenced discs are
//! excluded from indexing as their own games, so only the `.m3u` itself
//! becomes a game (`m3u_disc_paths`).
//! Still to come per the plan: background thread pool, FS watching.

pub mod archive;
pub mod hash;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

use crate::db::Db;
use crate::events::Event;
use crate::systems::SystemDef;
use crate::Result;

/// One logical scan target: a physical file, optionally paired with the
/// inner archive entry it represents. Several `Item`s can share the same
/// `path` when a zip expands into multiple games — they still share the
/// zip's own `(size, mtime)` quick key, since the zip is what's actually on
/// disk and what incremental scanning tracks.
struct Item {
    sys_idx: usize,
    path: std::path::PathBuf,
    in_archive: Option<String>,
    /// True only when this item came from a zip with more than one matching
    /// inner entry — the one case where the game is named after the inner
    /// ROM instead of the zip itself (spec: a single-match zip still names
    /// its game from the zip stem, since that's what players actually named).
    multi_entry: bool,
}

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

    // Collect candidate items first so progress has a stable total. A plain
    // ROM file or an opaque archive yields one item; a zip whose system has
    // real ROM extensions is enumerated here (no extraction) and can yield
    // one item per matching inner entry.
    let mut items: Vec<Item> = Vec::new();

    // System subfolders are matched case-insensitively (users name folders
    // "SNES" as often as "snes"; Android's storage is case-sensitive, so a
    // plain join would silently skip them). An unreadable/missing root is a
    // hard error rather than an empty walk — an empty walk would look like
    // "library emptied" and delete every indexed row below.
    let mut dirs_by_lower: HashMap<String, std::path::PathBuf> = HashMap::new();
    let mut subdir_names: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|e| crate::Error::Io {
        path: root.to_path_buf(),
        source: e,
    })?;
    for entry in entries {
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
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        dirs_by_lower.insert(name.to_ascii_lowercase(), entry.path());
        subdir_names.push(name);
    }

    let mut matched_any = false;
    for (idx, sys) in systems.iter().enumerate() {
        let Some(sys_root) = dirs_by_lower.get(&sys.slug.to_ascii_lowercase()) else {
            continue;
        };
        matched_any = true;
        let enumerate_zips = archive::should_enumerate(&sys.extensions);
        let rom_exts = archive::rom_extensions(&sys.extensions);
        for entry in WalkDir::new(sys_root).follow_links(false) {
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
            let path = entry.into_path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_default();
            if !sys.extensions.contains(&ext) {
                continue;
            }

            if ext == "zip" && enumerate_zips {
                match archive::list_rom_entries(&path, &rom_exts) {
                    Ok(names) if !names.is_empty() => {
                        let multi_entry = names.len() > 1;
                        for name in names {
                            items.push(Item {
                                sys_idx: idx,
                                path: path.clone(),
                                in_archive: Some(name),
                                multi_entry,
                            });
                        }
                    }
                    // No matching inner entries: fall back to indexing the
                    // zip itself, same as pre-archive-aware behavior.
                    Ok(_) => items.push(Item {
                        sys_idx: idx,
                        path,
                        in_archive: None,
                        multi_entry: false,
                    }),
                    Err(e) => {
                        sink(Event::Warning {
                            code: "scan.archive".into(),
                            context: format!("{}: {}", path.display(), e),
                        });
                        items.push(Item {
                            sys_idx: idx,
                            path,
                            in_archive: None,
                            multi_entry: false,
                        });
                    }
                }
            } else {
                items.push(Item {
                    sys_idx: idx,
                    path,
                    in_archive: None,
                    multi_entry: false,
                });
            }
        }
    }

    // Discs referenced by a `.m3u` playlist are indexed as part of that one
    // game, not as games of their own — drop them from `items` before the
    // main pass so incremental scanning (added/removed/unchanged) and
    // display naming never see them separately.
    let excluded = m3u_disc_paths(&items);
    if !excluded.is_empty() {
        items.retain(|item| item.in_archive.is_some() || !excluded.contains(&canon(&item.path)));
    }

    if !matched_any {
        sink(Event::Warning {
            code: "scan.no_system_dirs".into(),
            context: if subdir_names.is_empty() {
                format!("no subfolders under {}", root.display())
            } else {
                format!(
                    "no subfolder of {} matches a system slug; found: {}",
                    root.display(),
                    subdir_names.join(", ")
                )
            },
        });
    }

    let total = items.len() as u64;
    let mut summary = ScanSummary {
        added: 0,
        removed: 0,
        unchanged: 0,
    };
    // Identity for incremental scanning is (rel_path, in_archive), not just
    // rel_path: several items (inner zip entries) legitimately share a
    // rel_path.
    let mut seen: HashSet<(String, Option<String>)> = HashSet::new();
    let mut changed_systems: HashSet<i64> = HashSet::new();

    let tx = db.conn_mut().transaction()?;
    for (done, item) in items.iter().enumerate() {
        let sys = &systems[item.sys_idx];
        let path = &item.path;
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
        seen.insert((rel_path.clone(), item.in_archive.clone()));

        // `IS` (rather than `=`) correctly matches NULL in_archive too.
        let existing: Option<String> = tx
            .query_row(
                "SELECT quick_key FROM files WHERE library_id=?1 AND rel_path=?2 AND in_archive IS ?3",
                rusqlite::params![library_id, rel_path, item.in_archive],
                |r| r.get(0),
            )
            .ok();

        match existing {
            Some(k) if k == quick_key => summary.unchanged += 1,
            Some(_) => {
                tx.execute(
                    "UPDATE files SET size=?1, mtime=?2, quick_key=?3, crc32=NULL, md5=NULL
                     WHERE library_id=?4 AND rel_path=?5 AND in_archive IS ?6",
                    rusqlite::params![
                        size,
                        mtime,
                        quick_key,
                        library_id,
                        rel_path,
                        item.in_archive
                    ],
                )?;
                let system_id = system_db_id(&tx, &sys.slug)?;
                changed_systems.insert(system_id);
            }
            None => {
                let system_id = system_db_id(&tx, &sys.slug)?;
                // Multi-entry zips take the display name from the inner ROM
                // (its own identity); everything else — plain files, a
                // single-entry zip, an opaque archive — keeps naming games
                // after the file players actually see and name themselves.
                let name = match &item.in_archive {
                    Some(inner) if item.multi_entry => display_name(Path::new(inner)),
                    _ => display_name(path),
                };
                tx.execute(
                    "INSERT INTO games (system_id, canonical_name, sort_name) VALUES (?1, ?2, ?3)",
                    rusqlite::params![system_id, name, sort_key(&name)],
                )?;
                let game_id = tx.last_insert_rowid();
                tx.execute(
                    "INSERT INTO files (game_id, library_id, rel_path, size, mtime, quick_key, in_archive)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![game_id, library_id, rel_path, size, mtime, quick_key, item.in_archive],
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

    // Remove index rows whose files vanished from disk (games with no files
    // go too). Archive-aware: a removed zip's inner rows all disappear
    // together since none of them are in `seen` once the zip is gone.
    {
        let mut stmt =
            tx.prepare("SELECT id, rel_path, in_archive, game_id FROM files WHERE library_id=?1")?;
        let stale: Vec<(i64, i64)> = stmt
            .query_map([library_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .filter_map(|row| row.ok())
            .filter(|(_, rel, in_archive, _)| !seen.contains(&(rel.clone(), in_archive.clone())))
            .map(|(id, _, _, game_id)| (id, game_id))
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

/// Best-effort path normalization so an `.m3u` line and the disc file it
/// names compare equal despite case or separator differences; falls back to
/// the path as-is if the file can't be resolved (e.g. already excluded).
fn canon(p: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Every on-disk path referenced by an `.m3u` playlist among `items`, one
/// line per disc (blank lines and `#`-comments skipped), resolved relative
/// to the playlist's own directory — the convention every emulator that
/// reads `.m3u` follows.
fn m3u_disc_paths(items: &[Item]) -> HashSet<std::path::PathBuf> {
    let mut excluded = HashSet::new();
    for item in items {
        if item.in_archive.is_some() {
            continue;
        }
        let is_m3u = item
            .path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("m3u"));
        if !is_m3u {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&item.path) else {
            continue;
        };
        let Some(parent) = item.path.parent() else {
            continue;
        };
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            excluded.insert(canon(&parent.join(line.replace('\\', "/"))));
        }
    }
    excluded
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    /// snes-like: has both an archive extension and real ROM extensions, so
    /// its zips get enumerated.
    fn snes_system() -> SystemDef {
        SystemDef {
            slug: "snes".into(),
            name: "Super Nintendo".into(),
            sort_order: 0,
            extensions: vec!["sfc".into(), "zip".into()],
            ra_console_id: None,
            default_core: None,
            theme_key: None,
        }
    }

    /// arcade-like: extensions are all archive/disc-image containers, so its
    /// zips must NOT be enumerated — the zip itself is the game.
    fn arcade_system() -> SystemDef {
        SystemDef {
            slug: "arcade".into(),
            name: "Arcade".into(),
            sort_order: 0,
            extensions: vec!["zip".into(), "7z".into(), "chd".into()],
            ra_console_id: None,
            default_core: None,
            theme_key: None,
        }
    }

    /// Seed `libraries` + `systems` rows matching `defs`; returns the new library id.
    fn seed_library(db: &mut Db, root: &Path, defs: &[SystemDef]) -> i64 {
        let root_uri = root.to_string_lossy().replace('\\', "/");
        let conn = db.conn_mut();
        conn.execute(
            "INSERT INTO libraries (root_uri, name) VALUES (?1, 'Lib')",
            [&root_uri],
        )
        .unwrap();
        let library_id = conn.last_insert_rowid();
        for def in defs {
            conn.execute(
                "INSERT INTO systems (slug, name, extensions) VALUES (?1, ?2, ?3)",
                rusqlite::params![def.slug, def.name, def.extensions.join(",")],
            )
            .unwrap();
        }
        library_id
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    /// `(rel_path, in_archive, canonical_name)` for every indexed file, so
    /// tests can assert on both identity and naming.
    fn indexed_rows(db: &Db, library_id: i64) -> Vec<(String, Option<String>, String)> {
        let conn = db.conn();
        let mut stmt = conn
            .prepare(
                "SELECT f.rel_path, f.in_archive, g.canonical_name
                 FROM files f JOIN games g ON g.id = f.game_id
                 WHERE f.library_id = ?1
                 ORDER BY f.rel_path, f.in_archive",
            )
            .unwrap();
        stmt.query_map([library_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    }

    #[test]
    fn system_folder_matches_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        let snes_dir = dir.path().join("SNES");
        fs::create_dir_all(&snes_dir).unwrap();
        fs::write(snes_dir.join("Game.sfc"), b"data").unwrap();

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![snes_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        let summary = scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        assert_eq!(summary.added, 1);

        let rows = indexed_rows(&db, library_id);
        // rel_path keeps the on-disk casing so launches resolve the real file.
        assert_eq!(rows[0].0, "SNES/Game.sfc");
    }

    #[test]
    fn missing_root_is_an_error_not_a_wipe() {
        let dir = tempfile::tempdir().unwrap();
        let snes_dir = dir.path().join("snes");
        fs::create_dir_all(&snes_dir).unwrap();
        fs::write(snes_dir.join("Game.sfc"), b"data").unwrap();

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![snes_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);
        scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        assert_eq!(indexed_rows(&db, library_id).len(), 1);

        // Root gone (unmounted SD card, typo'd path): the scan must fail
        // loudly and leave the index untouched, not report "0 games".
        let gone = dir.path().join("nope");
        assert!(scan_library(&mut db, library_id, &gone, &defs, &mut |_| {}).is_err());
        assert_eq!(indexed_rows(&db, library_id).len(), 1);
    }

    #[test]
    fn no_matching_system_dirs_emits_warning() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Games")).unwrap();

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![snes_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        let mut events = Vec::new();
        scan_library(&mut db, library_id, dir.path(), &defs, &mut |e| {
            events.push(e)
        })
        .unwrap();
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Warning { code, context } if code == "scan.no_system_dirs" && context.contains("Games")
        )));
    }

    #[test]
    fn single_entry_zip_sets_in_archive_and_uses_zip_stem_name() {
        let dir = tempfile::tempdir().unwrap();
        let snes_dir = dir.path().join("snes");
        fs::create_dir_all(&snes_dir).unwrap();
        write_zip(
            &snes_dir.join("Super Mario World (USA).zip"),
            &[("rom.sfc", b"data")],
        );

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![snes_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        let summary = scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        assert_eq!(summary.added, 1);

        let rows = indexed_rows(&db, library_id);
        assert_eq!(rows.len(), 1);
        let (rel_path, in_archive, name) = &rows[0];
        assert_eq!(rel_path, "snes/Super Mario World (USA).zip");
        assert_eq!(in_archive.as_deref(), Some("rom.sfc"));
        // Zip stem, not the inner entry's name — players name their zips.
        assert_eq!(name, "Super Mario World (USA)");
    }

    #[test]
    fn multi_entry_zip_creates_one_game_per_entry() {
        let dir = tempfile::tempdir().unwrap();
        let snes_dir = dir.path().join("snes");
        fs::create_dir_all(&snes_dir).unwrap();
        write_zip(
            &snes_dir.join("compilation.zip"),
            &[("Game A.sfc", b"a"), ("Game B.sfc", b"b")],
        );

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![snes_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        let summary = scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        assert_eq!(summary.added, 2);

        let rows = indexed_rows(&db, library_id);
        assert_eq!(rows.len(), 2);
        let names: Vec<&str> = rows.iter().map(|(_, _, n)| n.as_str()).collect();
        assert!(names.contains(&"Game A"));
        assert!(names.contains(&"Game B"));
        for (rel_path, in_archive, _) in &rows {
            assert_eq!(rel_path, "snes/compilation.zip");
            assert!(in_archive.is_some());
        }
    }

    #[test]
    fn zip_with_no_matching_entries_falls_back_to_opaque_game() {
        let dir = tempfile::tempdir().unwrap();
        let snes_dir = dir.path().join("snes");
        fs::create_dir_all(&snes_dir).unwrap();
        write_zip(&snes_dir.join("extras.zip"), &[("readme.txt", b"n/a")]);

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![snes_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        let mut events = Vec::new();
        let summary = scan_library(&mut db, library_id, dir.path(), &defs, &mut |e| {
            events.push(e)
        })
        .unwrap();
        assert_eq!(summary.added, 1);
        assert!(!events
            .iter()
            .any(|e| matches!(e, Event::Warning { code, .. } if code == "scan.archive")));

        let rows = indexed_rows(&db, library_id);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, None);
        assert_eq!(rows[0].2, "extras");
    }

    #[test]
    fn unreadable_zip_falls_back_and_emits_scan_archive_warning() {
        let dir = tempfile::tempdir().unwrap();
        let snes_dir = dir.path().join("snes");
        fs::create_dir_all(&snes_dir).unwrap();
        fs::write(snes_dir.join("bad.zip"), b"not actually a zip file").unwrap();

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![snes_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        let mut events = Vec::new();
        let summary = scan_library(&mut db, library_id, dir.path(), &defs, &mut |e| {
            events.push(e)
        })
        .unwrap();
        assert_eq!(summary.added, 1);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::Warning { code, .. } if code == "scan.archive")));

        let rows = indexed_rows(&db, library_id);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, None);
    }

    #[test]
    fn arcade_style_system_indexes_zip_as_one_opaque_game() {
        let dir = tempfile::tempdir().unwrap();
        let arcade_dir = dir.path().join("arcade");
        fs::create_dir_all(&arcade_dir).unwrap();
        write_zip(
            &arcade_dir.join("pacman.zip"),
            &[("pacman.bin", b"rom"), ("cpu.bin", b"rom2")],
        );

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![arcade_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        let summary = scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        // should_enumerate is false for arcade (zip/7z/chd only) — the whole
        // zip stays one opaque game, exactly like pre-archive-aware behavior.
        assert_eq!(summary.added, 1);

        let rows = indexed_rows(&db, library_id);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, None);
        assert_eq!(rows[0].2, "pacman");
    }

    #[test]
    fn rescan_multi_entry_zip_plus_plain_file_is_unchanged_when_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let snes_dir = dir.path().join("snes");
        fs::create_dir_all(&snes_dir).unwrap();
        write_zip(
            &snes_dir.join("compilation.zip"),
            &[("Game A.sfc", b"a"), ("Game B.sfc", b"b")],
        );
        fs::write(snes_dir.join("Solo.sfc"), b"solo").unwrap();

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![snes_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        let first = scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        assert_eq!(first.added, 3);

        let second = scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        assert_eq!(second.added, 0);
        assert_eq!(second.unchanged, 3);
        assert_eq!(second.removed, 0);
    }

    /// psx-like: cue and m3u are both plain (non-archive) real extensions.
    fn psx_system() -> SystemDef {
        SystemDef {
            slug: "psx".into(),
            name: "PlayStation".into(),
            sort_order: 0,
            extensions: vec!["cue".into(), "m3u".into()],
            ra_console_id: None,
            default_core: None,
            theme_key: None,
        }
    }

    #[test]
    fn m3u_playlist_collapses_discs_into_one_game() {
        let dir = tempfile::tempdir().unwrap();
        let psx_dir = dir.path().join("psx");
        fs::create_dir_all(&psx_dir).unwrap();
        fs::write(psx_dir.join("Some Game (Disc 1).cue"), b"cue1").unwrap();
        fs::write(psx_dir.join("Some Game (Disc 2).cue"), b"cue2").unwrap();
        fs::write(
            psx_dir.join("Some Game.m3u"),
            "Some Game (Disc 1).cue\nSome Game (Disc 2).cue\n",
        )
        .unwrap();

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![psx_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        let summary = scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        assert_eq!(summary.added, 1);

        let rows = indexed_rows(&db, library_id);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "psx/Some Game.m3u");
        assert_eq!(rows[0].2, "Some Game");
    }

    #[test]
    fn discs_without_an_m3u_stay_separate_games() {
        let dir = tempfile::tempdir().unwrap();
        let psx_dir = dir.path().join("psx");
        fs::create_dir_all(&psx_dir).unwrap();
        fs::write(psx_dir.join("Solo Game (Disc 1).cue"), b"cue1").unwrap();
        fs::write(psx_dir.join("Solo Game (Disc 2).cue"), b"cue2").unwrap();

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![psx_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        let summary = scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        assert_eq!(summary.added, 2);
    }

    #[test]
    fn deleting_multi_entry_zip_removes_all_inner_games() {
        let dir = tempfile::tempdir().unwrap();
        let snes_dir = dir.path().join("snes");
        fs::create_dir_all(&snes_dir).unwrap();
        let zip_path = snes_dir.join("compilation.zip");
        write_zip(&zip_path, &[("Game A.sfc", b"a"), ("Game B.sfc", b"b")]);

        let mut db = Db::open_in_memory().unwrap();
        let defs = vec![snes_system()];
        let library_id = seed_library(&mut db, dir.path(), &defs);

        scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        assert_eq!(indexed_rows(&db, library_id).len(), 2);

        fs::remove_file(&zip_path).unwrap();
        let summary = scan_library(&mut db, library_id, dir.path(), &defs, &mut |_| {}).unwrap();
        assert_eq!(summary.removed, 2);
        assert_eq!(indexed_rows(&db, library_id).len(), 0);

        let game_count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))
            .unwrap();
        assert_eq!(game_count, 0);
    }
}
