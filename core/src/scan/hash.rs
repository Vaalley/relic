//! Lazy full-file hashing (PLAN.md §4.2).
//!
//! Scanning only ever computes a cheap `(size, mtime)` quick key; CRC32/MD5
//! are filled in later by [`hash_pending`] as a low-priority background job,
//! since they require reading whole file contents. Consumers: DAT-based
//! canonicalization (Phase 4) and RetroAchievements game identification
//! (Phase 6) both need a content hash to look games up. Note that RA itself
//! does *not* use plain MD5 for every console — several systems require
//! header-stripped or region-specific hashing rules (`rcheevos`); that
//! console-aware hashing lives in `modules/retroachievements` later and will
//! layer on top of this generic, whole-file hash rather than replace it.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use md5::{Digest, Md5};

/// Streaming read buffer size for [`hash_file`].
const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HashStats {
    pub hashed: u64,
    pub skipped: u64,
    pub failed: u64,
}

/// Hash `path` in one streaming pass, returning `(crc32, md5)` as lowercase
/// hex strings (8 and 32 characters respectively).
pub fn hash_file(path: &Path) -> std::io::Result<(String, String)> {
    let mut file = File::open(path)?;
    let mut crc = crc32fast::Hasher::new();
    let mut md5 = Md5::new();
    let mut buf = [0u8; CHUNK_SIZE];
    loop {
        let n = file.read(&mut buf)?;
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

/// Hash up to `limit` files whose `crc32`/`md5` are still unset, optionally
/// restricted to one library. File I/O runs with no transaction held; the
/// resulting rows are then applied in one short write transaction.
///
/// `skipped` is always 0 for now; it's reserved for a future size-cap policy
/// (e.g. skip multi-GB files until explicitly requested).
pub fn hash_pending(
    db: &mut crate::db::Db,
    library_id: Option<i64>,
    limit: usize,
    sink: &mut dyn FnMut(crate::events::Event),
) -> crate::Result<HashStats> {
    let mut stats = HashStats::default();

    // Candidate list: (file id, absolute path).
    let candidates: Vec<(i64, std::path::PathBuf)> = {
        let conn = db.conn();
        let mut rows: Vec<(i64, std::path::PathBuf)> = Vec::new();
        let mut push_rows = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> crate::Result<()> {
            let mut stmt = conn.prepare(sql)?;
            let mapped = stmt.query_map(params, |r| {
                let id: i64 = r.get(0)?;
                let root: String = r.get(1)?;
                let rel_path: String = r.get(2)?;
                Ok((id, std::path::PathBuf::from(root).join(rel_path)))
            })?;
            for row in mapped {
                rows.push(row?);
            }
            Ok(())
        };
        match library_id {
            Some(lib) => push_rows(
                "SELECT f.id, l.root_uri, f.rel_path FROM files f
                 JOIN libraries l ON l.id = f.library_id
                 WHERE f.crc32 IS NULL AND f.in_archive IS NULL AND f.library_id = ?1
                 LIMIT ?2",
                rusqlite::params![lib, limit as i64],
            )?,
            None => push_rows(
                "SELECT f.id, l.root_uri, f.rel_path FROM files f
                 JOIN libraries l ON l.id = f.library_id
                 WHERE f.crc32 IS NULL AND f.in_archive IS NULL
                 LIMIT ?1",
                rusqlite::params![limit as i64],
            )?,
        }
        rows
    };

    let mut hashed: Vec<(i64, String, String)> = Vec::with_capacity(candidates.len());
    for (file_id, path) in &candidates {
        match hash_file(path) {
            Ok((crc32, md5)) => hashed.push((*file_id, crc32, md5)),
            Err(e) => {
                stats.failed += 1;
                sink(crate::events::Event::Warning {
                    code: "hash.unreadable".into(),
                    context: format!("{}: {}", path.display(), e),
                });
            }
        }
    }

    let tx = db.conn_mut().transaction()?;
    for (file_id, crc32, md5) in &hashed {
        tx.execute(
            "UPDATE files SET crc32 = ?1, md5 = ?2 WHERE id = ?3",
            rusqlite::params![crc32, md5, file_id],
        )?;
        stats.hashed += 1;
    }
    tx.commit()?;

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn hash_file_matches_known_vectors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.bin");
        std::fs::write(&path, b"relic test fixture").unwrap();

        let (crc32, md5) = hash_file(&path).unwrap();
        // Verified independently (Python zlib.crc32 / hashlib.md5, and
        // .NET System.Security.Cryptography.MD5) against the same bytes.
        assert_eq!(crc32, "1200b7b5");
        assert_eq!(md5, "e0bfa1288941a3a223f67af71e2e45f5");
    }

    #[test]
    fn hash_file_missing_path_errors() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.bin");
        assert!(hash_file(&missing).is_err());
    }

    fn seed(db: &mut Db, root: &Path) -> i64 {
        let root_uri = root.to_string_lossy().replace('\\', "/");
        let conn = db.conn_mut();
        conn.execute(
            "INSERT INTO libraries (root_uri, name) VALUES (?1, 'Lib')",
            [&root_uri],
        )
        .unwrap();
        let library_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO systems (slug, name, extensions) VALUES ('snes', 'SNES', 'sfc')",
            [],
        )
        .unwrap();
        let system_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO games (system_id, canonical_name, sort_name) VALUES (?1, 'Game', 'game')",
            [system_id],
        )
        .unwrap();
        let game_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO files (game_id, library_id, rel_path, size, mtime, quick_key)
             VALUES (?1, ?2, 'present.rom', 19, 0, 'k1')",
            rusqlite::params![game_id, library_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files (game_id, library_id, rel_path, size, mtime, quick_key)
             VALUES (?1, ?2, 'missing.rom', 0, 0, 'k2')",
            rusqlite::params![game_id, library_id],
        )
        .unwrap();
        library_id
    }

    #[test]
    fn hash_pending_fills_rows_and_retries_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("present.rom"), b"relic test fixture").unwrap();
        // "missing.rom" is intentionally never created on disk.

        let mut db = Db::open_in_memory().unwrap();
        let library_id = seed(&mut db, dir.path());

        let mut events: Vec<crate::events::Event> = Vec::new();
        let stats = hash_pending(&mut db, Some(library_id), 10, &mut |e| events.push(e)).unwrap();

        assert_eq!(stats.hashed, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.skipped, 0);
        assert!(events.iter().any(
            |e| matches!(e, crate::events::Event::Warning { code, .. } if code == "hash.unreadable")
        ));

        let (crc32, md5): (Option<String>, Option<String>) = db
            .conn()
            .query_row(
                "SELECT crc32, md5 FROM files WHERE rel_path = 'present.rom'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(crc32.as_deref(), Some("1200b7b5"));
        assert_eq!(md5.as_deref(), Some("e0bfa1288941a3a223f67af71e2e45f5"));

        let missing_crc32: Option<String> = db
            .conn()
            .query_row(
                "SELECT crc32 FROM files WHERE rel_path = 'missing.rom'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(missing_crc32, None);

        // Second pass: the hashed row no longer qualifies (crc32 NOT NULL);
        // the missing row is retried and fails again, but nothing new hashes.
        let mut events2: Vec<crate::events::Event> = Vec::new();
        let stats2 = hash_pending(&mut db, Some(library_id), 10, &mut |e| events2.push(e)).unwrap();
        assert_eq!(stats2.hashed, 0);
        assert_eq!(stats2.failed, 1);
    }
}
