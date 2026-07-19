//! `gamelist.xml` (EmulationStation / ES-DE) import (Phase 1, PLAN.md §4.3/§5).
//!
//! Parsing is deliberately hand-rolled event-based `quick-xml`, not a serde
//! derive: the format is loosely specified in the wild (unknown tags from
//! forks/scrapers, missing fields, occasional bad entities), and this parser
//! will be fuzz-tested later (PLAN.md §8). Tolerance rules:
//! - unknown elements are skipped, not rejected;
//! - a `<game>` missing `<path>` is dropped, not an error;
//! - only `<game>` children of `<gameList>` are considered;
//! - a document is only an `Err` when the XML itself is unrecoverable.
//!
//! Import matches entries against already-scanned `files` rows by relative
//! path and writes a `metadata` row per game (source = "gamelist"); it never
//! creates `games`/`files` rows itself — that's the scanner's job.

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event as XmlEvent};
use quick_xml::{Reader, Writer};

/// One `<game>` entry from a gamelist.xml, tolerant of missing fields.
///
/// `path` is kept exactly as written in the source XML (e.g.
/// `"./Super Mario World (USA).sfc"`); normalization happens at import time.
/// `releasedate`/`players` are kept as raw ES-format strings (releasedate is
/// `YYYYMMDDTHHMMSS`; players is a range like `"1-2"`) since parsing them
/// further isn't needed for storage — the `metadata` table stores text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GamelistEntry {
    pub path: String,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub genre: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub releasedate: Option<String>,
    pub players: Option<String>,
    /// ES rating range is 0.0-1.0; kept as parsed float, invalid text dropped.
    pub rating: Option<f64>,
    pub image: Option<String>,
    pub marquee: Option<String>,
    pub video: Option<String>,
    pub favorite: bool,
    pub hidden: bool,
}

/// Local error type: a gamelist parse failure never needs to reach the FFI
/// boundary as a whole-crate error, only as an entry it can't use — callers
/// map this to whatever context (file path, warning event) they have.
#[derive(Debug, thiserror::Error)]
pub enum GamelistError {
    #[error("malformed gamelist.xml: {0}")]
    Xml(#[from] quick_xml::Error),
}

/// Parse a gamelist.xml document into its `<game>` entries.
///
/// Only `<game>` elements that are direct children of `<gameList>` are
/// collected; anything outside that (or a `<game>` missing `<path>`) is
/// silently dropped rather than failing the whole parse.
pub fn parse_gamelist(xml: &str) -> std::result::Result<Vec<GamelistEntry>, GamelistError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut game_list_depth: u32 = 0;
    let mut current: Option<GamelistEntry> = None;
    // Tracks the open-element stack while inside a <game>, so text captured
    // between an element's Start/End is attributed to the right field even
    // when unknown elements are nested around it.
    let mut field_stack: Vec<String> = Vec::new();
    let mut text_buf = String::new();

    loop {
        match reader.read_event() {
            Ok(XmlEvent::Eof) => break,
            Ok(XmlEvent::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "gameList" {
                    game_list_depth += 1;
                } else if game_list_depth > 0 && local == "game" && current.is_none() {
                    current = Some(GamelistEntry::default());
                }
                if current.is_some() {
                    field_stack.push(local);
                    text_buf.clear();
                }
            }
            Ok(XmlEvent::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if game_list_depth > 0 && local == "game" && current.is_none() {
                    // <game/> with no children at all: no <path>, drop it.
                } else if let Some(entry) = current.as_mut() {
                    assign_field(entry, &local, "");
                }
            }
            Ok(XmlEvent::Text(t)) => {
                if current.is_some() {
                    match t.unescape() {
                        Ok(txt) => text_buf.push_str(&txt),
                        // Bad entity in an otherwise well-formed document:
                        // fall back to the raw bytes rather than failing.
                        Err(_) => text_buf.push_str(&String::from_utf8_lossy(&t)),
                    }
                }
            }
            Ok(XmlEvent::CData(t)) => {
                if current.is_some() {
                    text_buf.push_str(&String::from_utf8_lossy(&t.into_inner()));
                }
            }
            Ok(XmlEvent::End(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "gameList" {
                    game_list_depth = game_list_depth.saturating_sub(1);
                }
                if local == "game" {
                    if let Some(entry) = current.take() {
                        if !entry.path.is_empty() {
                            entries.push(entry);
                        }
                    }
                    field_stack.clear();
                    text_buf.clear();
                } else if current.is_some()
                    && field_stack.last().map(String::as_str) == Some(local.as_str())
                {
                    field_stack.pop();
                    let value = std::mem::take(&mut text_buf);
                    if let Some(entry) = current.as_mut() {
                        assign_field(entry, &local, value.trim());
                    }
                }
            }
            // Comments, processing instructions, declarations, doctypes: ignored.
            Ok(_) => {}
            Err(e) => return Err(GamelistError::from(e)),
        }
    }

    Ok(entries)
}

fn local_name(qname: &[u8]) -> String {
    // Strip an XML namespace prefix ("es:name" -> "name"); gamelist.xml has
    // no real namespaces in practice, but this keeps a stray prefix from
    // silently turning a known field into an "unknown" one.
    let bytes = match qname.iter().position(|&b| b == b':') {
        Some(idx) => &qname[idx + 1..],
        None => qname,
    };
    String::from_utf8_lossy(bytes).into_owned()
}

fn assign_field(entry: &mut GamelistEntry, field: &str, value: &str) {
    match field {
        "path" => entry.path = value.to_string(),
        "name" => set_opt(&mut entry.name, value),
        "desc" => set_opt(&mut entry.desc, value),
        "genre" => set_opt(&mut entry.genre, value),
        "developer" => set_opt(&mut entry.developer, value),
        "publisher" => set_opt(&mut entry.publisher, value),
        "releasedate" => set_opt(&mut entry.releasedate, value),
        "players" => set_opt(&mut entry.players, value),
        "rating" => {
            if let Ok(r) = value.parse::<f64>() {
                entry.rating = Some(r);
            }
        }
        "image" => set_opt(&mut entry.image, value),
        "marquee" => set_opt(&mut entry.marquee, value),
        "video" => set_opt(&mut entry.video, value),
        "favorite" => entry.favorite = is_truthy(value),
        "hidden" => entry.hidden = is_truthy(value),
        // Unknown element: tolerated, ignored.
        _ => {}
    }
}

fn set_opt(slot: &mut Option<String>, value: &str) {
    if !value.is_empty() {
        *slot = Some(value.to_string());
    }
}

fn is_truthy(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value == "1"
}

/// Per-import counts, returned to the caller (surfaced as an event/log line
/// by whoever drives the import; this module doesn't know about `events`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GamelistImportStats {
    pub matched: u64,
    pub unmatched: u64,
}

/// Match parsed entries against already-scanned `files` rows and upsert
/// `metadata` (source = "gamelist"). Does not create `games`/`files` rows —
/// an entry with no matching file is counted `unmatched` and skipped.
///
/// `gamelist_rel_dir` is the directory containing the gamelist.xml, relative
/// to the library root (`""` for the library root itself, e.g. `"snes"` for
/// the usual per-system subfolder layout) — joined with each entry's `path`
/// (after stripping a leading `./`) to reconstruct the `files.rel_path` key.
pub fn import_gamelist(
    db: &mut crate::db::Db,
    library_id: i64,
    system_slug: &str,
    gamelist_rel_dir: &str,
    entries: &[GamelistEntry],
) -> crate::Result<GamelistImportStats> {
    let tx = db.conn_mut().transaction()?;

    let system_id: i64 = tx
        .query_row("SELECT id FROM systems WHERE slug=?1", [system_slug], |r| {
            r.get(0)
        })
        .map_err(|_| crate::Error::UnknownSystem(system_slug.to_string()))?;

    let mut stats = GamelistImportStats::default();

    for entry in entries {
        let rel_path = normalize_rel_path(gamelist_rel_dir, &entry.path);

        let game_id: Option<i64> = tx
            .query_row(
                "SELECT f.game_id FROM files f
                 JOIN games g ON g.id = f.game_id
                 WHERE f.library_id = ?1 AND f.rel_path = ?2 AND g.system_id = ?3
                 LIMIT 1",
                rusqlite::params![library_id, rel_path, system_id],
                |r| r.get(0),
            )
            .ok();

        let Some(game_id) = game_id else {
            stats.unmatched += 1;
            continue;
        };

        tx.execute(
            "INSERT INTO metadata
                (game_id, source, title, description, genre, developer, publisher,
                 release_date, players, rating)
             VALUES (?1, 'gamelist', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(game_id, source) DO UPDATE SET
                title=excluded.title,
                description=excluded.description,
                genre=excluded.genre,
                developer=excluded.developer,
                publisher=excluded.publisher,
                release_date=excluded.release_date,
                players=excluded.players,
                rating=excluded.rating",
            rusqlite::params![
                game_id,
                entry.name,
                entry.desc,
                entry.genre,
                entry.developer,
                entry.publisher,
                entry.releasedate,
                entry.players,
                entry.rating,
            ],
        )?;

        if let Some(name) = &entry.name {
            tx.execute(
                "UPDATE games SET canonical_name = ?1, sort_name = ?2 WHERE id = ?3",
                rusqlite::params![name, sort_key(name), game_id],
            )?;
        }

        stats.matched += 1;
    }

    tx.commit()?;
    Ok(stats)
}

/// Render every game of one system back to `gamelist.xml` (interop out —
/// the inverse of [`import_gamelist`]). Prefers `metadata` rows with
/// `source = 'gamelist'` for text fields (title/desc/genre/…), falling back
/// to the scanned `canonical_name` when no such row exists; favorite/hidden
/// come from `user_data`, the precious per-user table.
///
/// Media paths (image/marquee/video) are deliberately not written yet: this
/// crate's media cache is content-addressed, not the relative-path-next-to-
/// gamelist.xml convention ES/ES-DE expect, and that mapping isn't wired up.
pub fn export_gamelist(
    db: &crate::db::Db,
    library_id: i64,
    system_slug: &str,
    gamelist_rel_dir: &str,
) -> crate::Result<String> {
    let conn = db.conn();
    let system_id: i64 = conn
        .query_row("SELECT id FROM systems WHERE slug=?1", [system_slug], |r| {
            r.get(0)
        })
        .map_err(|_| crate::Error::UnknownSystem(system_slug.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT f.rel_path, g.canonical_name,
                m.title, m.description, m.genre, m.developer, m.publisher,
                m.release_date, m.players, m.rating,
                COALESCE(u.favorite, 0), COALESCE(u.hidden, 0)
         FROM games g
         JOIN files f ON f.game_id = g.id AND f.library_id = ?1
         LEFT JOIN metadata m ON m.game_id = g.id AND m.source = 'gamelist'
         LEFT JOIN user_data u ON u.game_id = g.id
         WHERE g.system_id = ?2
         ORDER BY g.sort_name",
    )?;

    let rows: Vec<ExportRow> = stmt
        .query_map(rusqlite::params![library_id, system_id], |r| {
            Ok(ExportRow {
                rel_path: r.get(0)?,
                canonical_name: r.get(1)?,
                title: r.get(2)?,
                desc: r.get(3)?,
                genre: r.get(4)?,
                developer: r.get(5)?,
                publisher: r.get(6)?,
                release_date: r.get(7)?,
                players: r.get(8)?,
                rating: r.get(9)?,
                favorite: r.get::<_, i64>(10)? != 0,
                hidden: r.get::<_, i64>(11)? != 0,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut writer = Writer::new_with_indent(std::io::Cursor::new(Vec::new()), b' ', 4);
    // Writing to an in-memory Vec<u8> cannot fail, so these are unwrapped
    // rather than threaded through crate::Error.
    writer
        .write_event(XmlEvent::Decl(BytesDecl::new("1.0", None, None)))
        .unwrap();
    writer
        .write_event(XmlEvent::Start(BytesStart::new("gameList")))
        .unwrap();

    for row in &rows {
        writer
            .write_event(XmlEvent::Start(BytesStart::new("game")))
            .unwrap();
        write_field(
            &mut writer,
            "path",
            &export_path(gamelist_rel_dir, &row.rel_path),
        );
        write_field(
            &mut writer,
            "name",
            row.title.as_deref().unwrap_or(&row.canonical_name),
        );
        if let Some(v) = &row.desc {
            write_field(&mut writer, "desc", v);
        }
        if let Some(v) = &row.genre {
            write_field(&mut writer, "genre", v);
        }
        if let Some(v) = &row.developer {
            write_field(&mut writer, "developer", v);
        }
        if let Some(v) = &row.publisher {
            write_field(&mut writer, "publisher", v);
        }
        if let Some(v) = &row.release_date {
            write_field(&mut writer, "releasedate", v);
        }
        if let Some(v) = &row.players {
            write_field(&mut writer, "players", v);
        }
        if let Some(v) = row.rating {
            write_field(&mut writer, "rating", &v.to_string());
        }
        if row.favorite {
            write_field(&mut writer, "favorite", "true");
        }
        if row.hidden {
            write_field(&mut writer, "hidden", "true");
        }
        writer
            .write_event(XmlEvent::End(BytesEnd::new("game")))
            .unwrap();
    }

    writer
        .write_event(XmlEvent::End(BytesEnd::new("gameList")))
        .unwrap();
    let bytes = writer.into_inner().into_inner();
    Ok(String::from_utf8(bytes).expect("quick_xml writer always produces valid utf8"))
}

struct ExportRow {
    rel_path: String,
    canonical_name: String,
    title: Option<String>,
    desc: Option<String>,
    genre: Option<String>,
    developer: Option<String>,
    publisher: Option<String>,
    release_date: Option<String>,
    players: Option<String>,
    rating: Option<f64>,
    favorite: bool,
    hidden: bool,
}

fn write_field(writer: &mut Writer<std::io::Cursor<Vec<u8>>>, tag: &str, value: &str) {
    writer
        .write_event(XmlEvent::Start(BytesStart::new(tag)))
        .unwrap();
    writer
        .write_event(XmlEvent::Text(BytesText::new(value)))
        .unwrap();
    writer
        .write_event(XmlEvent::End(BytesEnd::new(tag)))
        .unwrap();
}

/// Inverse of `normalize_rel_path`: strip the gamelist's own directory
/// prefix off a stored `files.rel_path` and restore the `./`-relative form
/// ES/ES-DE gamelist.xml entries use.
fn export_path(gamelist_rel_dir: &str, rel_path: &str) -> String {
    let dir = gamelist_rel_dir.trim_matches('/');
    let stripped = if dir.is_empty() {
        rel_path
    } else {
        rel_path
            .strip_prefix(dir)
            .and_then(|s| s.strip_prefix('/'))
            .unwrap_or(rel_path)
    };
    format!("./{stripped}")
}

fn normalize_rel_path(gamelist_rel_dir: &str, raw_path: &str) -> String {
    let raw = raw_path.trim().replace('\\', "/");
    let stripped = raw.strip_prefix("./").unwrap_or(&raw);
    let dir = gamelist_rel_dir.trim_matches('/');
    if dir.is_empty() {
        stripped.to_string()
    } else {
        format!("{dir}/{stripped}")
    }
}

/// Naive sort key; duplicated from `scan::sort_key` on purpose (module
/// boundary — see task constraints) rather than importing across modules.
/// DAT-based canonicalization replaces this in Phase 4.
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
    use std::path::PathBuf;

    fn fixture_path(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join(rel)
    }

    #[test]
    fn parses_mini_snes_fixture() {
        let xml = std::fs::read_to_string(fixture_path("mini/snes/gamelist.xml")).unwrap();
        let entries = parse_gamelist(&xml).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.path, "./Super Mario World (USA).sfc");
        assert_eq!(e.name.as_deref(), Some("Super Mario World"));
        assert_eq!(e.genre.as_deref(), Some("Platform"));
        assert_eq!(e.releasedate.as_deref(), Some("19910821T000000"));
        assert!(e.desc.as_deref().unwrap().contains("Bowser"));
    }

    #[test]
    fn skips_unknown_elements_and_decodes_entities() {
        let xml = r#"<?xml version="1.0"?>
            <gameList>
                <game>
                    <path>./Foo &amp; Bar.zip</path>
                    <name>Foo &amp; Bar</name>
                    <futureField><nested>ignored</nested></futureField>
                    <rating>0.8</rating>
                    <favorite>true</favorite>
                </game>
            </gameList>"#;
        let entries = parse_gamelist(xml).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.path, "./Foo & Bar.zip");
        assert_eq!(e.name.as_deref(), Some("Foo & Bar"));
        assert_eq!(e.rating, Some(0.8));
        assert!(e.favorite);
    }

    #[test]
    fn skips_entries_missing_path() {
        let xml = r#"<gameList>
                <game>
                    <name>No Path Here</name>
                </game>
                <game>
                    <path>./ok.sfc</path>
                    <name>OK</name>
                </game>
            </gameList>"#;
        let entries = parse_gamelist(xml).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "./ok.sfc");
    }

    #[test]
    fn ignores_games_outside_gamelist_element() {
        let xml = r#"<root>
                <game><path>./stray.sfc</path></game>
                <gameList>
                    <game><path>./real.sfc</path></game>
                </gameList>
            </root>"#;
        let entries = parse_gamelist(xml).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "./real.sfc");
    }

    #[test]
    fn malformed_xml_is_an_error() {
        for xml in [
            "<gameList><game><path>./a.sfc</path></game></gamelistX>",
            "<gameList><game><path>./a.sfc</pathX></game></gameList>",
            "<gameList><game attr=\"unterminated><path>./a.sfc</path></game></gameList>",
        ] {
            assert!(parse_gamelist(xml).is_err(), "expected error for {xml}");
        }
    }

    fn setup_db_with_one_file() -> (crate::db::Db, i64, i64) {
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
                 VALUES (1, 1, 'Super Mario World (USA)', 'super mario world (usa)')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO files (id, game_id, library_id, rel_path, size, mtime, quick_key)
                 VALUES (1, 1, 1, 'snes/Super Mario World (USA).sfc', 100, 0, '100:0')",
                [],
            )
            .unwrap();
        }
        (db, 1, 1)
    }

    #[test]
    fn import_matches_file_and_updates_metadata_and_canonical_name() {
        let (mut db, library_id, game_id) = setup_db_with_one_file();
        let entries = vec![GamelistEntry {
            path: "./Super Mario World (USA).sfc".to_string(),
            name: Some("Super Mario World".to_string()),
            desc: Some("A classic platformer.".to_string()),
            genre: Some("Platform".to_string()),
            releasedate: Some("19910821T000000".to_string()),
            players: Some("1-2".to_string()),
            rating: Some(0.9),
            ..Default::default()
        }];

        let stats = import_gamelist(&mut db, library_id, "snes", "snes", &entries).unwrap();
        assert_eq!(stats.matched, 1);
        assert_eq!(stats.unmatched, 0);

        let conn = db.conn_mut();
        let (title, genre, rating): (String, String, f64) = conn
            .query_row(
                "SELECT title, genre, rating FROM metadata WHERE game_id=?1 AND source='gamelist'",
                [game_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(title, "Super Mario World");
        assert_eq!(genre, "Platform");
        assert_eq!(rating, 0.9);

        let (canonical, sort): (String, String) = conn
            .query_row(
                "SELECT canonical_name, sort_name FROM games WHERE id=?1",
                [game_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(canonical, "Super Mario World");
        assert_eq!(sort, "super mario world");
    }

    #[test]
    fn import_counts_unmatched_entries() {
        let (mut db, library_id, _game_id) = setup_db_with_one_file();
        let entries = vec![GamelistEntry {
            path: "./Does Not Exist.sfc".to_string(),
            name: Some("Ghost".to_string()),
            ..Default::default()
        }];

        let stats = import_gamelist(&mut db, library_id, "snes", "snes", &entries).unwrap();
        assert_eq!(stats.matched, 0);
        assert_eq!(stats.unmatched, 1);
    }

    #[test]
    fn import_unknown_system_slug_errors() {
        let (mut db, library_id, _game_id) = setup_db_with_one_file();
        let err = import_gamelist(&mut db, library_id, "nope", "snes", &[]).unwrap_err();
        assert!(matches!(err, crate::Error::UnknownSystem(_)));
    }

    #[test]
    fn export_round_trips_an_imported_entry() {
        let (mut db, library_id, _game_id) = setup_db_with_one_file();
        let entries = vec![GamelistEntry {
            path: "./Super Mario World (USA).sfc".to_string(),
            name: Some("Super Mario World".to_string()),
            desc: Some("A classic platformer.".to_string()),
            genre: Some("Platform".to_string()),
            releasedate: Some("19910821T000000".to_string()),
            players: Some("1-2".to_string()),
            rating: Some(0.9),
            ..Default::default()
        }];
        import_gamelist(&mut db, library_id, "snes", "snes", &entries).unwrap();

        let xml = export_gamelist(&db, library_id, "snes", "snes").unwrap();
        let reparsed = parse_gamelist(&xml).unwrap();
        assert_eq!(reparsed.len(), 1);
        let e = &reparsed[0];
        assert_eq!(e.path, "./Super Mario World (USA).sfc");
        assert_eq!(e.name.as_deref(), Some("Super Mario World"));
        assert_eq!(e.desc.as_deref(), Some("A classic platformer."));
        assert_eq!(e.genre.as_deref(), Some("Platform"));
        assert_eq!(e.releasedate.as_deref(), Some("19910821T000000"));
        assert_eq!(e.players.as_deref(), Some("1-2"));
        assert_eq!(e.rating, Some(0.9));
    }

    #[test]
    fn export_falls_back_to_canonical_name_without_metadata() {
        let (db, library_id, _game_id) = setup_db_with_one_file();
        let xml = export_gamelist(&db, library_id, "snes", "snes").unwrap();
        let reparsed = parse_gamelist(&xml).unwrap();
        assert_eq!(reparsed.len(), 1);
        assert_eq!(reparsed[0].name.as_deref(), Some("Super Mario World (USA)"));
        assert_eq!(reparsed[0].desc, None);
    }

    #[test]
    fn export_includes_favorite_flag_from_user_data() {
        let (mut db, library_id, game_id) = setup_db_with_one_file();
        db.conn_mut()
            .execute(
                "INSERT INTO user_data (game_id, favorite) VALUES (?1, 1)",
                [game_id],
            )
            .unwrap();

        let xml = export_gamelist(&db, library_id, "snes", "snes").unwrap();
        let reparsed = parse_gamelist(&xml).unwrap();
        assert!(reparsed[0].favorite);
    }

    #[test]
    fn export_unknown_system_slug_errors() {
        let (db, library_id, _game_id) = setup_db_with_one_file();
        let err = export_gamelist(&db, library_id, "nope", "snes").unwrap_err();
        assert!(matches!(err, crate::Error::UnknownSystem(_)));
    }
}
