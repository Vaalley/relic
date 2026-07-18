//! Zip archive enumeration (PLAN.md §4.2 "archive-aware: `.zip`/`.7z` are
//! enumerated without extraction").
//!
//! Scope this round: **zip only**. The `zip` crate only speaks the ZIP
//! format; `.7z` files are still indexed (as one opaque game per the
//! pre-archive-aware behavior — see `should_enumerate` below) but never
//! opened. 7z enumeration is future work, gated behind adding a 7z-capable
//! dependency (PLAN.md §2.1 names `sevenz-rust`).
//!
//! Entries are listed straight from the zip central directory — no bytes are
//! extracted to disk. [`hash_entry`] streams a single entry's bytes in place
//! for the lazy hasher (`scan::hash`); it never extracts either.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use md5::{Digest, Md5};
use zip::ZipArchive;

/// Streaming read buffer size, matching `scan::hash::CHUNK_SIZE`.
const CHUNK_SIZE: usize = 64 * 1024;

fn zip_err(e: zip::result::ZipError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
}

/// List inner entry names of `zip_path` whose lowercase extension is in
/// `allowed_exts`. Directories are skipped. `allowed_exts` should already
/// exclude archive/container extensions (zip, 7z, chd) — pass a system's
/// real ROM extensions, not its whole `extensions` list.
pub fn list_rom_entries(zip_path: &Path, allowed_exts: &[String]) -> std::io::Result<Vec<String>> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(zip_err)?;

    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(zip_err)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let ext = Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if allowed_exts.contains(&ext) {
            entries.push(name);
        }
    }
    Ok(entries)
}

/// Hash one inner entry of `zip_path` in a single streaming pass, returning
/// `(crc32, md5)` hex strings — the same shape as `scan::hash::hash_file`,
/// so callers can treat archive members and plain files uniformly.
pub fn hash_entry(zip_path: &Path, inner_name: &str) -> std::io::Result<(String, String)> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(zip_err)?;
    let mut entry = archive.by_name(inner_name).map_err(zip_err)?;

    let mut crc = crc32fast::Hasher::new();
    let mut md5 = Md5::new();
    let mut buf = [0u8; CHUNK_SIZE];
    loop {
        let n = entry.read(&mut buf)?;
        if n == 0 {
            break;
        }
        crc.update(&buf[..n]);
        md5.update(&buf[..n]);
    }
    Ok((
        format!("{:08x}", crc.finalize()),
        format!("{:x}", md5.finalize()),
    ))
}

/// Whether a system's zips should be enumerated for inner ROM entries at
/// all. Requires `zip` in the extension list *and* at least one allowed
/// extension that isn't itself an archive/disc-image container (zip, 7z,
/// chd) — otherwise the zip/7z/chd *is* the game (e.g. arcade), and
/// enumerating it would explode one romset into its constituent parts.
pub fn should_enumerate(extensions: &[String]) -> bool {
    extensions.iter().any(|e| e == "zip")
        && extensions
            .iter()
            .any(|e| !matches!(e.as_str(), "zip" | "7z" | "chd"))
}

/// A system's real ROM extensions: its extension list minus the
/// archive/disc-image container extensions themselves.
pub fn rom_extensions(extensions: &[String]) -> Vec<String> {
    extensions
        .iter()
        .filter(|e| !matches!(e.as_str(), "zip" | "7z" | "chd"))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn should_enumerate_true_for_mixed_extension_systems() {
        let snes = vec!["sfc".to_string(), "smc".to_string(), "zip".to_string()];
        assert!(should_enumerate(&snes));
    }

    #[test]
    fn should_enumerate_false_for_archive_only_systems() {
        let arcade = vec!["zip".to_string(), "7z".to_string(), "chd".to_string()];
        assert!(!should_enumerate(&arcade));
    }

    #[test]
    fn should_enumerate_false_without_zip() {
        let no_zip = vec!["sfc".to_string(), "smc".to_string()];
        assert!(!should_enumerate(&no_zip));
    }

    #[test]
    fn rom_extensions_strips_archive_exts() {
        let exts = vec![
            "sfc".to_string(),
            "smc".to_string(),
            "zip".to_string(),
            "7z".to_string(),
        ];
        assert_eq!(
            rom_extensions(&exts),
            vec!["sfc".to_string(), "smc".to_string()]
        );
    }

    #[test]
    fn list_rom_entries_finds_matching_extensions_only() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("game.zip");
        write_zip(
            &zip_path,
            &[
                ("Game.sfc", b"rom bytes"),
                ("readme.txt", b"not a rom"),
                ("sub/Other.sfc", b"more rom bytes"),
            ],
        );

        let mut entries = list_rom_entries(&zip_path, &["sfc".to_string()]).unwrap();
        entries.sort();
        assert_eq!(
            entries,
            vec!["Game.sfc".to_string(), "sub/Other.sfc".to_string()]
        );
    }

    #[test]
    fn list_rom_entries_skips_directories() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("game.zip");
        let file = File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        zip.add_directory("sub/", SimpleFileOptions::default())
            .unwrap();
        zip.start_file("sub/Game.sfc", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"rom bytes").unwrap();
        zip.finish().unwrap();

        let entries = list_rom_entries(&zip_path, &["sfc".to_string()]).unwrap();
        assert_eq!(entries, vec!["sub/Game.sfc".to_string()]);
    }

    #[test]
    fn list_rom_entries_empty_when_nothing_matches() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("game.zip");
        write_zip(&zip_path, &[("readme.txt", b"not a rom")]);

        let entries = list_rom_entries(&zip_path, &["sfc".to_string()]).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn list_rom_entries_errors_on_unreadable_zip() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("not-a-zip.zip");
        std::fs::write(&bogus, b"this is not a zip file").unwrap();

        assert!(list_rom_entries(&bogus, &["sfc".to_string()]).is_err());
    }

    #[test]
    fn hash_entry_matches_hash_file_on_same_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("game.zip");
        write_zip(&zip_path, &[("Game.sfc", b"relic test fixture")]);

        let (crc32, md5) = hash_entry(&zip_path, "Game.sfc").unwrap();
        // Same bytes/vectors as scan::hash::hash_file_matches_known_vectors.
        assert_eq!(crc32, "1200b7b5");
        assert_eq!(md5, "e0bfa1288941a3a223f67af71e2e45f5");
    }

    #[test]
    fn hash_entry_errors_on_missing_entry() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("game.zip");
        write_zip(&zip_path, &[("Game.sfc", b"relic test fixture")]);

        assert!(hash_entry(&zip_path, "Nope.sfc").is_err());
    }
}
