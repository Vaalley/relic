//! No-Intro/Redump DAT (Logiqx `datafile` XML) parsing and hash matching
//! (Phase 4, PLAN.md §4.3 "DAT matching for canonical names").
//!
//! Same tolerance philosophy as `gamelist.rs`: unknown elements/attributes
//! are skipped, a `<rom>` missing `crc` is dropped rather than erroring the
//! whole parse, and only a genuinely malformed XML document is an `Err`.
//! Both `<game>` (No-Intro/Redump) and `<machine>` (MAME-style DATs) entry
//! tags are accepted.
//!
//! Matching is CRC32-only for v1: `files.crc32` is already populated by the
//! lazy hash pipeline (`scan::hash`), so no extra hashing pass is needed
//! here. A DAT's `md5`/`sha1` attributes are parsed for a future fallback
//! but not matched against yet — this crate doesn't store `sha1`.

use quick_xml::events::{BytesStart, Event as XmlEvent};
use quick_xml::Reader;

/// One `<rom>` entry inside a DAT `<game>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DatRom {
    pub name: String,
    pub size: Option<u64>,
    /// Lowercase, zero-padded to 8 hex digits — same format as `files.crc32`.
    pub crc: Option<String>,
    pub md5: Option<String>,
}

/// One `<game>`/`<machine>` entry from a DAT file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DatEntry {
    /// The `name` attribute — usually already the canonical release name
    /// for No-Intro/Redump (region tags and all), e.g.
    /// `"Super Mario World (USA)"`.
    pub name: String,
    /// `<description>` child, when present; some DATs duplicate `name` here,
    /// others put a cleaner display name — preferred over `name` when set.
    pub description: Option<String>,
    pub roms: Vec<DatRom>,
}

#[derive(Debug, thiserror::Error)]
pub enum DatError {
    #[error("malformed DAT file: {0}")]
    Xml(#[from] quick_xml::Error),
}

/// Parse a DAT document into its `<game>`/`<machine>` entries.
pub fn parse_dat(xml: &str) -> std::result::Result<Vec<DatEntry>, DatError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut current: Option<DatEntry> = None;
    let mut in_description = false;
    let mut text_buf = String::new();

    loop {
        match reader.read_event() {
            Ok(XmlEvent::Eof) => break,
            Ok(XmlEvent::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if (local == "game" || local == "machine") && current.is_none() {
                    current = Some(DatEntry {
                        name: get_attr(&e, "name").unwrap_or_default(),
                        ..Default::default()
                    });
                } else if local == "description" && current.is_some() {
                    in_description = true;
                    text_buf.clear();
                } else if local == "rom" && current.is_some() {
                    push_rom(&mut current, &e);
                }
            }
            Ok(XmlEvent::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "rom" && current.is_some() {
                    push_rom(&mut current, &e);
                }
            }
            Ok(XmlEvent::Text(t)) => {
                if in_description {
                    match t.unescape() {
                        Ok(txt) => text_buf.push_str(&txt),
                        Err(_) => text_buf.push_str(&String::from_utf8_lossy(&t)),
                    }
                }
            }
            Ok(XmlEvent::End(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "description" && in_description {
                    in_description = false;
                    let value = std::mem::take(&mut text_buf);
                    if let Some(entry) = current.as_mut() {
                        let trimmed = value.trim();
                        if !trimmed.is_empty() {
                            entry.description = Some(trimmed.to_string());
                        }
                    }
                } else if local == "game" || local == "machine" {
                    if let Some(entry) = current.take() {
                        if !entry.name.is_empty() {
                            entries.push(entry);
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(e) => return Err(DatError::from(e)),
        }
    }

    Ok(entries)
}

fn push_rom(current: &mut Option<DatEntry>, e: &BytesStart) {
    let Some(entry) = current.as_mut() else {
        return;
    };
    entry.roms.push(DatRom {
        name: get_attr(e, "name").unwrap_or_default(),
        size: get_attr(e, "size").and_then(|s| s.parse().ok()),
        crc: get_attr(e, "crc").as_deref().and_then(normalize_crc),
        md5: get_attr(e, "md5"),
    });
}

fn get_attr(e: &BytesStart, key: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key.as_bytes())
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// Lowercase and zero-pad to 8 hex digits — the format `files.crc32` is
/// stored in (`scan::hash::hash_file`). Non-hex or empty values are dropped
/// rather than erroring the whole entry.
fn normalize_crc(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 8 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("{:0>8}", trimmed.to_ascii_lowercase()))
}

fn local_name(qname: &[u8]) -> String {
    let bytes = match qname.iter().position(|&b| b == b':') {
        Some(idx) => &qname[idx + 1..],
        None => qname,
    };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Per-match counts, mirroring `GamelistImportStats`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DatMatchStats {
    pub matched: u64,
    pub unmatched: u64,
}

/// Match DAT entries against already-hashed `files.crc32` rows for one
/// system, updating `games.canonical_name`/`sort_name` and a `metadata` row
/// (`source = 'dat'`) on each hit. An entry with no usable CRC in the DAT,
/// or whose CRC matches no file (not yet hashed, or genuinely absent from
/// this library), is counted `unmatched` rather than erroring the batch.
pub fn match_dat(
    db: &mut crate::db::Db,
    system_slug: &str,
    entries: &[DatEntry],
) -> crate::Result<DatMatchStats> {
    let tx = db.conn_mut().transaction()?;
    let system_id: i64 = tx
        .query_row("SELECT id FROM systems WHERE slug=?1", [system_slug], |r| {
            r.get(0)
        })
        .map_err(|_| crate::Error::UnknownSystem(system_slug.to_string()))?;

    let mut stats = DatMatchStats::default();
    for entry in entries {
        let Some(crc) = entry.roms.iter().find_map(|r| r.crc.clone()) else {
            stats.unmatched += 1;
            continue;
        };

        let game_id: Option<i64> = tx
            .query_row(
                "SELECT f.game_id FROM files f
                 JOIN games g ON g.id = f.game_id
                 WHERE f.crc32 = ?1 AND g.system_id = ?2
                 LIMIT 1",
                rusqlite::params![crc, system_id],
                |r| r.get(0),
            )
            .ok();

        let Some(game_id) = game_id else {
            stats.unmatched += 1;
            continue;
        };

        let name = entry
            .description
            .clone()
            .unwrap_or_else(|| entry.name.clone());
        tx.execute(
            "INSERT INTO metadata (game_id, source, title) VALUES (?1, 'dat', ?2)
             ON CONFLICT(game_id, source) DO UPDATE SET title=excluded.title",
            rusqlite::params![game_id, name],
        )?;
        tx.execute(
            "UPDATE games SET canonical_name = ?1, sort_name = ?2 WHERE id = ?3",
            rusqlite::params![name, sort_key(&name), game_id],
        )?;
        stats.matched += 1;
    }

    tx.commit()?;
    Ok(stats)
}

/// Naive sort key; duplicated from `scan::sort_key`/`gamelist::sort_key` on
/// purpose (module boundary), same as the rest of this crate's metadata code.
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

    const SAMPLE_DAT: &str = r#"<?xml version="1.0"?>
<!DOCTYPE datafile PUBLIC "-//Logiqx//DTD ROM Management Datafile//EN" "http://www.logiqx.com/Dats/datafile.dtd">
<datafile>
    <header>
        <name>Nintendo - Super Nintendo Entertainment System</name>
    </header>
    <game name="Super Mario World (USA)">
        <description>Super Mario World (USA)</description>
        <rom name="Super Mario World (USA).sfc" size="524288" crc="B19ED489" md5="cdd3c8c8"/>
    </game>
    <game name="No Hash Game (USA)">
        <rom name="No Hash Game (USA).sfc" size="1024"/>
    </game>
</datafile>"#;

    #[test]
    fn parses_sample_dat_with_crc_and_description() {
        let entries = parse_dat(SAMPLE_DAT).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "Super Mario World (USA)");
        assert_eq!(
            entries[0].description.as_deref(),
            Some("Super Mario World (USA)")
        );
        assert_eq!(entries[0].roms.len(), 1);
        assert_eq!(entries[0].roms[0].crc.as_deref(), Some("b19ed489"));
        assert_eq!(entries[0].roms[0].size, Some(524288));
        assert_eq!(entries[1].roms[0].crc, None);
    }

    #[test]
    fn machine_tag_is_accepted_like_game() {
        let xml = r#"<datafile><machine name="pacman"><rom name="pacman.bin" crc="12345678"/></machine></datafile>"#;
        let entries = parse_dat(xml).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "pacman");
    }

    #[test]
    fn unknown_elements_and_missing_name_are_tolerated() {
        let xml = r#"<datafile>
            <header><name>Some DAT</name><future><nested>x</nested></future></header>
            <game><rom name="no name attr" crc="deadbeef"/></game>
            <game name="Valid"><rom name="v.rom" crc="cafe"/></game>
        </datafile>"#;
        let entries = parse_dat(xml).unwrap();
        // The nameless <game> is dropped; only "Valid" survives.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Valid");
        // A short CRC is zero-padded to 8 digits.
        assert_eq!(entries[0].roms[0].crc.as_deref(), Some("0000cafe"));
    }

    #[test]
    fn malformed_xml_is_an_error() {
        assert!(parse_dat("<datafile><game name=\"x\"").is_err());
    }

    fn setup_db_with_hashed_file() -> (crate::db::Db, i64) {
        let mut db = crate::db::Db::open_in_memory().unwrap();
        {
            let conn = db.conn_mut();
            conn.execute(
                "INSERT INTO libraries (id, root_uri, name) VALUES (1, 'file:///lib', 'Lib')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO systems (id, slug, name, extensions) VALUES (1, 'snes', 'SNES', 'sfc')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO games (id, system_id, canonical_name, sort_name)
                 VALUES (1, 1, 'smw', 'smw')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO files (id, game_id, library_id, rel_path, size, mtime, quick_key, crc32)
                 VALUES (1, 1, 1, 'snes/smw.sfc', 100, 0, '100:0', 'b19ed489')",
                [],
            )
            .unwrap();
        }
        (db, 1)
    }

    #[test]
    fn match_dat_updates_canonical_name_on_crc_hit() {
        let (mut db, game_id) = setup_db_with_hashed_file();
        let entries = parse_dat(SAMPLE_DAT).unwrap();

        let stats = match_dat(&mut db, "snes", &entries).unwrap();
        assert_eq!(stats.matched, 1);
        assert_eq!(stats.unmatched, 1);

        let (canonical, title): (String, String) = db
            .conn()
            .query_row(
                "SELECT g.canonical_name, m.title FROM games g
                 JOIN metadata m ON m.game_id = g.id AND m.source = 'dat'
                 WHERE g.id = ?1",
                [game_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(canonical, "Super Mario World (USA)");
        assert_eq!(title, "Super Mario World (USA)");
    }

    #[test]
    fn match_dat_unknown_system_errors() {
        let (mut db, _game_id) = setup_db_with_hashed_file();
        let err = match_dat(&mut db, "nope", &[]).unwrap_err();
        assert!(matches!(err, crate::Error::UnknownSystem(_)));
    }

    #[test]
    fn match_dat_no_hash_yet_counts_unmatched_not_error() {
        let mut db = crate::db::Db::open_in_memory().unwrap();
        db.conn_mut()
            .execute(
                "INSERT INTO systems (id, slug, name, extensions) VALUES (1, 'snes', 'SNES', 'sfc')",
                [],
            )
            .unwrap();
        let entries = parse_dat(SAMPLE_DAT).unwrap();
        let stats = match_dat(&mut db, "snes", &entries).unwrap();
        assert_eq!(stats.matched, 0);
        assert_eq!(stats.unmatched, 2);
    }
}
