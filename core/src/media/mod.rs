//! Local media discovery + content-addressed thumbnail cache (Phase 1).
//!
//! Implements docs/media-conventions.md: for each indexed ROM, artwork is
//! discovered offline through four priority tiers (Relic-native dir >
//! gamelist.xml paths > ES-DE layout > same-folder), then image kinds flow
//! into a SHA-256 content-addressed cache of downscaled thumbnails. The DB
//! stores source path + hash only; the cache is disposable and rebuildable.
//! Videos are indexed but served from their source path (spec open question
//! §7.2 resolved as "no cache copy" for now).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sha2::Digest;

use crate::db::Db;
use crate::events::Event;
use crate::metadata::gamelist;
use crate::Result;

/// Longest edge of cached thumbnails, in pixels (PLAN.md §8 grid budget).
const THUMB_MAX_DIM: u32 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Boxart,
    Screenshot,
    Marquee,
    Video,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaKind::Boxart => "boxart",
            MediaKind::Screenshot => "screenshot",
            MediaKind::Marquee => "marquee",
            MediaKind::Video => "video",
        }
    }

    /// ES-DE directory name for this kind (spec §4.3).
    fn es_de_dir(self) -> &'static str {
        match self {
            MediaKind::Boxart => "covers",
            MediaKind::Screenshot => "screenshots",
            MediaKind::Marquee => "marquees",
            MediaKind::Video => "videos",
        }
    }

    fn is_image(self) -> bool {
        !matches!(self, MediaKind::Video)
    }

    /// Permitted extensions, best-quality first (spec §5 tie-breaking).
    fn quality_order(self) -> &'static [&'static str] {
        if self.is_image() {
            &["png", "webp", "jpg", "jpeg"]
        } else {
            &["mp4", "webm"]
        }
    }
}

const ALL_KINDS: [MediaKind; 4] = [
    MediaKind::Boxart,
    MediaKind::Screenshot,
    MediaKind::Marquee,
    MediaKind::Video,
];

#[derive(Debug, Clone)]
pub struct MediaRow {
    pub kind: String,
    pub source: String,
    pub cache_hash: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MediaStats {
    pub discovered: u64,
    pub thumbnails_cached: u64,
    pub failed: u64,
}

#[derive(Debug, Clone)]
struct Discovered {
    kind: MediaKind,
    source: &'static str,
    path: PathBuf,
}

/// Case-insensitive per-directory listing cache so discovery does one
/// read_dir per directory regardless of how many games live in it.
#[derive(Default)]
struct DirIndex {
    dirs: HashMap<PathBuf, HashMap<String, Vec<String>>>,
}

impl DirIndex {
    fn entries(&mut self, dir: &Path) -> &HashMap<String, Vec<String>> {
        if !self.dirs.contains_key(dir) {
            let mut by_stem: HashMap<String, Vec<String>> = HashMap::new();
            if let Ok(rd) = std::fs::read_dir(dir) {
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if let Some(stem) = Path::new(&name).file_stem() {
                        by_stem
                            .entry(stem.to_string_lossy().to_lowercase())
                            .or_default()
                            .push(name);
                    }
                }
            }
            self.dirs.insert(dir.to_path_buf(), by_stem);
        }
        &self.dirs[dir]
    }

    /// Best match for `stem` in `dir` among `quality`-ordered extensions:
    /// lowest quality rank wins, then lexicographic byte order (spec §5).
    fn best_match(&mut self, dir: &Path, stem_lower: &str, quality: &[&str]) -> Option<PathBuf> {
        let candidates = self.entries(dir).get(stem_lower)?;
        candidates
            .iter()
            .filter_map(|name| {
                let ext = Path::new(name)
                    .extension()?
                    .to_string_lossy()
                    .to_lowercase();
                let rank = quality.iter().position(|q| *q == ext)?;
                Some((rank, name.clone()))
            })
            .min()
            .map(|(_, name)| dir.join(name))
    }
}

/// Media referenced by a system's gamelist.xml, keyed by ROM stem (tier 2).
type GamelistMedia = HashMap<String, Vec<(MediaKind, PathBuf)>>;

fn gamelist_media_for_dir(system_dir: &Path) -> GamelistMedia {
    let mut map: GamelistMedia = HashMap::new();
    let gamelist_path = system_dir.join("gamelist.xml");
    let Ok(xml) = std::fs::read_to_string(&gamelist_path) else {
        return map;
    };
    let Ok(entries) = gamelist::parse_gamelist(&xml) else {
        return map;
    };
    for entry in entries {
        let rom_stem = Path::new(entry.path.trim_start_matches("./"))
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase());
        let Some(rom_stem) = rom_stem else { continue };
        let refs = [
            (MediaKind::Boxart, &entry.image),
            (MediaKind::Marquee, &entry.marquee),
            (MediaKind::Video, &entry.video),
        ];
        for (kind, rel) in refs {
            if let Some(rel) = rel {
                let p = system_dir.join(rel.trim_start_matches("./"));
                map.entry(rom_stem.clone()).or_default().push((kind, p));
            }
        }
    }
    map
}

/// Discover media for one ROM through the four priority tiers (spec §4).
fn discover_for_rom(
    root: &Path,
    system_slug: &str,
    rom_rel_path: &str,
    gamelist_media: &GamelistMedia,
    idx: &mut DirIndex,
) -> Vec<Discovered> {
    let rom_abs = root.join(rom_rel_path);
    let stem_lower = match rom_abs.file_stem() {
        Some(s) => s.to_string_lossy().to_lowercase(),
        None => return Vec::new(),
    };
    let system_dir = root.join(system_slug);
    let mut found = Vec::new();

    for kind in ALL_KINDS {
        // Tier 1: <slug>/.relic-media/<kind>/
        let native_dir = system_dir.join(".relic-media").join(kind.as_str());
        if let Some(path) = idx.best_match(&native_dir, &stem_lower, kind.quality_order()) {
            found.push(Discovered {
                kind,
                source: "relic_native",
                path,
            });
            continue;
        }
        // Tier 2: gamelist.xml <image>/<marquee>/<video>
        if let Some(refs) = gamelist_media.get(&stem_lower) {
            if let Some((_, path)) = refs.iter().find(|(k, p)| *k == kind && p.is_file()) {
                found.push(Discovered {
                    kind,
                    source: "gamelist",
                    path: path.clone(),
                });
                continue;
            }
        }
        // Tier 3: <slug>/media/<covers|screenshots|marquees|videos>/
        let es_dir = system_dir.join("media").join(kind.es_de_dir());
        if let Some(path) = idx.best_match(&es_dir, &stem_lower, kind.quality_order()) {
            found.push(Discovered {
                kind,
                source: "es_de",
                path,
            });
            continue;
        }
        // Tier 4: same folder as the ROM, boxart only, png/jpg/jpeg only.
        if kind == MediaKind::Boxart {
            if let Some(dir) = rom_abs.parent() {
                if let Some(path) = idx.best_match(dir, &stem_lower, &["png", "jpg", "jpeg"]) {
                    found.push(Discovered {
                        kind,
                        source: "same_folder",
                        path,
                    });
                }
            }
        }
    }
    found
}

/// SHA-256 of a file's contents, lowercase hex.
fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Ensure a downscaled thumbnail for `source` exists in the cache; returns
/// the content hash. Cache layout: <cache_dir>/<hh>/<hash>.png (spec §6).
fn ensure_thumbnail(cache_dir: &Path, source: &Path) -> std::io::Result<String> {
    let hash = sha256_file(source)?;
    let bucket = cache_dir.join(&hash[..2]);
    let cache_file = bucket.join(format!("{hash}.png"));
    if cache_file.is_file() {
        return Ok(hash);
    }
    std::fs::create_dir_all(&bucket)?;
    let img = image::open(source).map_err(std::io::Error::other)?;
    let img = if img.width().max(img.height()) > THUMB_MAX_DIM {
        img.resize(
            THUMB_MAX_DIM,
            THUMB_MAX_DIM,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    img.save_with_format(&cache_file, image::ImageFormat::Png)
        .map_err(std::io::Error::other)?;
    Ok(hash)
}

/// Discover and index media for every game in a library, refreshing the
/// thumbnail cache when `cache_dir` is available. Existing rows whose source
/// is unchanged and whose cache file still exists are skipped cheaply.
pub(crate) fn refresh_media(
    db: &mut Db,
    library_id: i64,
    root: &Path,
    cache_dir: Option<&Path>,
    sink: &mut dyn FnMut(Event),
) -> Result<MediaStats> {
    let mut stats = MediaStats::default();
    let mut idx = DirIndex::default();

    let files: Vec<(i64, String)> = {
        let mut stmt = db.conn().prepare(
            "SELECT game_id, rel_path FROM files
             WHERE library_id = ?1 AND in_archive IS NULL",
        )?;
        let rows = stmt
            .query_map([library_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };

    // Group by system dir so each gamelist.xml is parsed at most once.
    let mut gamelist_cache: HashMap<String, GamelistMedia> = HashMap::new();

    let tx = db.conn_mut().transaction()?;
    for (game_id, rel_path) in files {
        let Some(slug) = rel_path.split('/').next().map(str::to_owned) else {
            continue;
        };
        let gamelist_media = gamelist_cache
            .entry(slug.clone())
            .or_insert_with(|| gamelist_media_for_dir(&root.join(&slug)));

        for d in discover_for_rom(root, &slug, &rel_path, gamelist_media, &mut idx) {
            let source_path = d.path.to_string_lossy().replace('\\', "/");

            // Fast path: same source already indexed and cache intact.
            let existing: Option<(String, String)> = tx
                .query_row(
                    "SELECT cache_hash, COALESCE(source_path,'') FROM media
                     WHERE game_id=?1 AND kind=?2 AND source=?3",
                    rusqlite::params![game_id, d.kind.as_str(), d.source],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .ok();
            if let (Some((hash, sp)), Some(cache)) = (&existing, cache_dir) {
                if *sp == source_path
                    && (!d.kind.is_image()
                        || cache
                            .join(&hash[..2.min(hash.len())])
                            .join(format!("{hash}.png"))
                            .is_file())
                {
                    stats.discovered += 1;
                    continue;
                }
            }

            let cache_hash = match (cache_dir, d.kind.is_image()) {
                (Some(cache), true) => match ensure_thumbnail(cache, &d.path) {
                    Ok(hash) => {
                        stats.thumbnails_cached += 1;
                        hash
                    }
                    Err(e) => {
                        stats.failed += 1;
                        sink(Event::Warning {
                            code: "media.thumbnail".into(),
                            context: format!("{}: {e}", d.path.display()),
                        });
                        continue;
                    }
                },
                _ => String::new(),
            };

            // One winning row per (game, kind): clear lower-priority sources.
            tx.execute(
                "DELETE FROM media WHERE game_id=?1 AND kind=?2",
                rusqlite::params![game_id, d.kind.as_str()],
            )?;
            tx.execute(
                "INSERT INTO media (game_id, kind, source, cache_hash, source_path)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![game_id, d.kind.as_str(), d.source, cache_hash, source_path],
            )?;
            stats.discovered += 1;
        }
    }
    tx.commit()?;
    Ok(stats)
}

pub(crate) fn media_for_game(db: &Db, game_id: i64) -> Result<Vec<MediaRow>> {
    let mut stmt = db.conn().prepare(
        "SELECT kind, source, cache_hash, COALESCE(source_path,'')
         FROM media WHERE game_id=?1 ORDER BY kind",
    )?;
    let rows = stmt
        .query_map([game_id], |r| {
            Ok(MediaRow {
                kind: r.get(0)?,
                source: r.get(1)?,
                cache_hash: r.get(2)?,
                source_path: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, w: u32, h: u32) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
        img.save_with_format(path, image::ImageFormat::Png).unwrap();
    }

    #[test]
    fn tier_priority_and_tiebreak() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let snes = root.join("snes");
        std::fs::create_dir_all(&snes).unwrap();
        std::fs::write(snes.join("Game A.sfc"), b"x").unwrap();

        // Tier 4 (same folder), then tier 3, then tier 1 — highest wins.
        write_png(&snes.join("Game A.png"), 8, 8);
        let mut idx = DirIndex::default();
        let d = discover_for_rom(root, "snes", "snes/Game A.sfc", &HashMap::new(), &mut idx);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].source, "same_folder");

        write_png(&snes.join("media/covers/game a.png"), 8, 8);
        let mut idx = DirIndex::default();
        let d = discover_for_rom(root, "snes", "snes/Game A.sfc", &HashMap::new(), &mut idx);
        assert_eq!(
            d[0].source, "es_de",
            "case-insensitive es_de beats same_folder"
        );

        write_png(&snes.join(".relic-media/boxart/GAME A.webp"), 8, 8);
        let mut idx = DirIndex::default();
        let d = discover_for_rom(root, "snes", "snes/Game A.sfc", &HashMap::new(), &mut idx);
        assert_eq!(d[0].source, "relic_native");

        // Tie-break: png beats webp within one tier.
        write_png(&snes.join(".relic-media/boxart/Game A.png"), 8, 8);
        let mut idx = DirIndex::default();
        let d = discover_for_rom(root, "snes", "snes/Game A.sfc", &HashMap::new(), &mut idx);
        assert!(d[0].path.to_string_lossy().ends_with("Game A.png"));
    }

    #[test]
    fn thumbnail_cache_is_content_addressed_and_downscaled() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("big.png");
        write_png(&src, 1024, 512);
        let cache = dir.path().join("cache");

        let h1 = ensure_thumbnail(&cache, &src).unwrap();
        let h2 = ensure_thumbnail(&cache, &src).unwrap();
        assert_eq!(h1, h2);
        let cached = cache.join(&h1[..2]).join(format!("{h1}.png"));
        let thumb = image::open(&cached).unwrap();
        assert_eq!(thumb.width().max(thumb.height()), THUMB_MAX_DIM);
    }
}
