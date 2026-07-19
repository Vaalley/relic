//! Best-effort emulator auto-detection for the first-run flow (PLAN.md
//! Phase 2 "first-run wizard... detect emulators"). Scans `PATH` for a
//! small, curated list of well-known emulator executable names.
//!
//! Deliberately conservative: only `PATH`-resolvable executables are found.
//! macOS `.app` bundles under `/Applications` are not scanned (they're
//! rarely on `PATH`) — a user on macOS still adds their emulator manually
//! via "Browse…". Extending detection to bundle paths is future work, not
//! a silent gap papered over here.

use std::path::PathBuf;

/// One detectable emulator: a display name plus the executable stems to
/// look for (checked as given, and with `.exe` appended on Windows if the
/// bare name isn't found). Grounded in `core/data/systems/*.toml`'s
/// `default_core` values (RetroArch libretro cores) plus the standalone
/// alternatives most likely to already be installed instead of RetroArch.
const CANDIDATES: &[(&str, &[&str])] = &[
    ("RetroArch", &["retroarch"]),
    ("Dolphin", &["dolphin-emu"]),
    ("PPSSPP", &["ppsspp", "PPSSPPSDL", "PPSSPPQt"]),
    ("mGBA", &["mgba-qt", "mgba"]),
    ("Snes9x", &["snes9x-gtk", "snes9x"]),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedEmulator {
    pub name: String,
    pub exec: String,
}

/// Scan `path_var` (a `PATH`-style, platform-separator-joined string) for
/// the first match of each candidate. Takes `PATH` as a parameter rather
/// than reading the environment directly so this is unit-testable without
/// mutating global process state.
pub fn detect_emulators(path_var: &str) -> Vec<DetectedEmulator> {
    let dirs: Vec<PathBuf> = std::env::split_paths(path_var).collect();

    let mut found = Vec::new();
    for (name, stems) in CANDIDATES {
        if let Some(exec) = find_first(&dirs, stems) {
            found.push(DetectedEmulator {
                name: name.to_string(),
                exec,
            });
        }
    }
    found
}

fn find_first(dirs: &[PathBuf], stems: &[&str]) -> Option<String> {
    for dir in dirs {
        for stem in stems {
            let candidate = dir.join(stem);
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
            if cfg!(windows) {
                let with_exe = dir.join(format!("{stem}.exe"));
                if with_exe.is_file() {
                    return Some(with_exe.to_string_lossy().into_owned());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_nothing_on_empty_path() {
        assert!(detect_emulators("").is_empty());
    }

    #[test]
    fn finds_an_executable_present_on_path() {
        let dir = tempfile::tempdir().unwrap();
        let exec_name = if cfg!(windows) {
            "retroarch.exe"
        } else {
            "retroarch"
        };
        let exec_path = dir.path().join(exec_name);
        std::fs::write(&exec_path, b"#!/bin/sh\n").unwrap();

        let found = detect_emulators(&dir.path().to_string_lossy());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "RetroArch");
        assert_eq!(found[0].exec, exec_path.to_string_lossy());
    }

    #[test]
    fn ignores_a_directory_that_merely_shares_the_executable_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("retroarch")).unwrap();

        assert!(detect_emulators(&dir.path().to_string_lossy()).is_empty());
    }

    #[test]
    fn later_path_entries_are_found_too() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let exec_name = if cfg!(windows) { "mgba.exe" } else { "mgba" };
        std::fs::write(dir_b.path().join(exec_name), b"x").unwrap();

        let path_var = std::env::join_paths([dir_a.path(), dir_b.path()])
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let found = detect_emulators(&path_var);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "mGBA");
    }
}
