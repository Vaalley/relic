//! Manifest and directory validation (spec section 7).
//!
//! Phase 5 groundwork, PLAN.md section 6 layer 1. `validate_manifest` checks
//! everything derivable from the TOML text alone; `validate_dir` additionally
//! checks that referenced sound assets resolve inside the theme folder and
//! exist on disk. Both are purely offline (no network, no reads outside the
//! given path).

use crate::model::Theme;
use std::path::Path;

/// Issue severity. Errors reject the theme at load; warnings are surfaced
/// but the theme still loads (spec section 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A single validation finding. `code` is a stable kebab-case identifier
/// that shells/CLI can match on; `message` is human-facing.
#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
}

impl Issue {
    fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: code.to_string(),
            message: message.into(),
        }
    }

    fn warning(code: &str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.to_string(),
            message: message.into(),
        }
    }
}

/// Supported `format_version` range, baked into the crate at build time
/// (spec section 8). The current draft targets version 1 only.
const MIN_FORMAT_VERSION: i64 = 1;
const MAX_FORMAT_VERSION: i64 = 1;

/// Permitted sound asset extensions (spec section 7).
const SOUND_EXTS: &[&str] = &["wav", "ogg", "flac", "mp3"];

// Known-key sets, used to detect forward-compat drift (spec section 7:
// unknown keys are warnings, never errors).
const TOP_KEYS: &[&str] = &["theme", "colors", "typography", "shape", "sounds"];
const THEME_KEYS: &[&str] = &["name", "author", "version", "format_version", "description"];
const COLORS_KEYS: &[&str] = &["dark", "light"];
const COLOR_SET_KEYS: &[&str] = &["bg", "surface", "text", "text_dim", "accent", "favorite"];
const TYPOGRAPHY_KEYS: &[&str] = &["font_family", "scale"];
const SHAPE_KEYS: &[&str] = &["radius"];
const SOUNDS_KEYS: &[&str] = &["move", "select", "back"];

/// Validate a manifest string. Returns every issue found (errors and
/// warnings). Pure: reads no files, makes no network calls.
pub fn validate_manifest(toml_src: &str) -> Vec<Issue> {
    let mut issues = Vec::new();

    let value: toml::Value = match toml::from_str(toml_src) {
        Ok(v) => v,
        Err(e) => {
            issues.push(Issue::error(
                "unparseable-toml",
                format!("theme.toml is not valid TOML: {e}"),
            ));
            return issues;
        }
    };
    let root = match value.as_table() {
        Some(t) => t,
        None => {
            issues.push(Issue::error(
                "unparseable-toml",
                "theme.toml root must be a table",
            ));
            return issues;
        }
    };

    for key in root.keys() {
        if !TOP_KEYS.contains(&key.as_str()) {
            issues.push(Issue::warning(
                "unknown-key",
                format!("unknown top-level key `{key}`"),
            ));
        }
    }

    check_theme(root, &mut issues);
    check_colors(root, &mut issues);
    check_typography(root, &mut issues);
    check_shape(root, &mut issues);
    check_sounds(root, &mut issues);

    issues
}

fn check_theme(root: &toml::Table, issues: &mut Vec<Issue>) {
    let theme = root.get("theme").and_then(|v| v.as_table());
    match theme {
        Some(theme) => {
            for key in theme.keys() {
                if !THEME_KEYS.contains(&key.as_str()) {
                    issues.push(Issue::warning(
                        "unknown-key",
                        format!("unknown key in `[theme]`: `{key}`"),
                    ));
                }
            }
            if theme.get("name").and_then(|v| v.as_str()).is_none() {
                issues.push(Issue::error(
                    "missing-name",
                    "required field `theme.name` is missing or not a string",
                ));
            }
            match theme.get("format_version").and_then(|v| v.as_integer()) {
                None => issues.push(Issue::error(
                    "missing-format-version",
                    "required field `theme.format_version` is missing or not an integer",
                )),
                Some(v) if !(MIN_FORMAT_VERSION..=MAX_FORMAT_VERSION).contains(&v) => {
                    issues.push(Issue::error(
                        "unsupported-format-version",
                        format!(
                            "theme targets format v{v}, this Relic supports v{MIN_FORMAT_VERSION}\u{2013}v{MAX_FORMAT_VERSION}"
                        ),
                    ));
                }
                _ => {}
            }
        }
        None => {
            issues.push(Issue::error(
                "missing-theme-table",
                "required `[theme]` table is missing",
            ));
            issues.push(Issue::error(
                "missing-name",
                "required field `theme.name` is missing",
            ));
            issues.push(Issue::error(
                "missing-format-version",
                "required field `theme.format_version` is missing",
            ));
        }
    }
}

fn check_colors(root: &toml::Table, issues: &mut Vec<Issue>) {
    let Some(colors) = root.get("colors").and_then(|v| v.as_table()) else {
        return;
    };
    for key in colors.keys() {
        if !COLORS_KEYS.contains(&key.as_str()) {
            issues.push(Issue::warning(
                "unknown-key",
                format!("unknown key in `[colors]`: `{key}`"),
            ));
        }
    }
    for variant in ["dark", "light"] {
        let Some(set) = colors.get(variant).and_then(|v| v.as_table()) else {
            continue;
        };
        for key in set.keys() {
            if !COLOR_SET_KEYS.contains(&key.as_str()) {
                issues.push(Issue::warning(
                    "unknown-key",
                    format!("unknown key in `[colors.{variant}]`: `{key}`"),
                ));
            }
        }
        for key in COLOR_SET_KEYS {
            let Some(val) = set.get(*key) else { continue };
            match val.as_str() {
                Some(s) => {
                    if !is_valid_hex(s) {
                        issues.push(Issue::error(
                            "bad-color",
                            format!("`colors.{variant}.{key}` is not a valid hex color: `{s}`"),
                        ));
                    }
                }
                None => issues.push(Issue::error(
                    "bad-color",
                    format!("`colors.{variant}.{key}` must be a string"),
                )),
            }
        }
    }
}

fn check_typography(root: &toml::Table, issues: &mut Vec<Issue>) {
    let Some(typo) = root.get("typography").and_then(|v| v.as_table()) else {
        return;
    };
    for key in typo.keys() {
        if !TYPOGRAPHY_KEYS.contains(&key.as_str()) {
            issues.push(Issue::warning(
                "unknown-key",
                format!("unknown key in `[typography]`: `{key}`"),
            ));
        }
    }
    if let Some(scale) = typo.get("scale") {
        // TOML floats and integers are distinct value kinds; accept both as
        // numbers for the scale check.
        let as_f64 = scale
            .as_float()
            .or_else(|| scale.as_integer().map(|i| i as f64));
        match as_f64 {
            None => issues.push(Issue::error(
                "scale-out-of-range",
                "`typography.scale` must be a number",
            )),
            Some(f) if f <= 0.0 => issues.push(Issue::error(
                "scale-out-of-range",
                "`typography.scale` must be > 0",
            )),
            Some(f) if !(0.5..=3.0).contains(&f) => issues.push(Issue::warning(
                "scale-out-of-range",
                format!("`typography.scale` ({f}) outside recommended [0.5, 3.0]"),
            )),
            _ => {}
        }
    }
}

fn check_shape(root: &toml::Table, issues: &mut Vec<Issue>) {
    let Some(shape) = root.get("shape").and_then(|v| v.as_table()) else {
        return;
    };
    for key in shape.keys() {
        if !SHAPE_KEYS.contains(&key.as_str()) {
            issues.push(Issue::warning(
                "unknown-key",
                format!("unknown key in `[shape]`: `{key}`"),
            ));
        }
    }
    if let Some(radius) = shape.get("radius") {
        match radius.as_integer() {
            Some(i) if i < 0 => issues.push(Issue::error(
                "radius-negative",
                "`shape.radius` must be >= 0",
            )),
            None => issues.push(Issue::error(
                "radius-negative",
                "`shape.radius` must be an integer",
            )),
            _ => {}
        }
    }
}

fn check_sounds(root: &toml::Table, issues: &mut Vec<Issue>) {
    let Some(sounds) = root.get("sounds").and_then(|v| v.as_table()) else {
        return;
    };
    for key in sounds.keys() {
        if !SOUNDS_KEYS.contains(&key.as_str()) {
            issues.push(Issue::warning(
                "unknown-key",
                format!("unknown key in `[sounds]`: `{key}`"),
            ));
        }
    }
    for key in SOUNDS_KEYS {
        if let Some(val) = sounds.get(*key) {
            if !val.is_str() {
                issues.push(Issue::error(
                    "bad-sound",
                    format!("`sounds.{key}` must be a string"),
                ));
            }
        }
    }
}

/// `#RGB`, `#RGBA`, `#RRGGBB`, or `#RRGGBBAA`, case-insensitive (spec 5.1).
fn is_valid_hex(s: &str) -> bool {
    let rest = match s.strip_prefix('#') {
        Some(r) => r,
        None => return false,
    };
    let len = rest.len();
    if !matches!(len, 3 | 4 | 6 | 8) {
        return false;
    }
    rest.chars().all(|c| c.is_ascii_hexdigit())
}

/// Parse a manifest into a typed `Theme`. Returns `Err(issues)` iff any
/// error-severity issue is present; otherwise `Ok(theme)`. Warnings are
/// not returned here — use `validate_manifest` to see them.
pub fn parse_theme(toml_src: &str) -> Result<Theme, Vec<Issue>> {
    let issues = validate_manifest(toml_src);
    if issues.iter().any(|i| i.severity == Severity::Error) {
        return Err(issues);
    }
    toml::from_str::<Theme>(toml_src).map_err(|e| {
        vec![Issue::error(
            "unparseable-toml",
            format!("failed to deserialize theme: {e}"),
        )]
    })
}

/// Load a theme directory into a typed [`Theme`], for a shell that wants to
/// actually apply a user-selected theme (not just validate it). Returns
/// `Err(issues)` iff [`validate_dir`] finds any error-severity issue —
/// warnings are dropped, same split as [`parse_theme`] vs
/// `validate_manifest`. Re-reads `theme.toml` after validation rather than
/// threading the source through, since themes are tiny and this isn't a hot
/// path (loaded once at startup, or on a resume-triggered mtime check).
pub fn load_theme_dir(theme_dir: &Path) -> Result<Theme, Vec<Issue>> {
    let issues = validate_dir(theme_dir);
    if issues.iter().any(|i| i.severity == Severity::Error) {
        return Err(issues);
    }
    let src = std::fs::read_to_string(theme_dir.join("theme.toml")).map_err(|e| {
        vec![Issue::error(
            "missing-manifest",
            format!("cannot read `theme.toml`: {e}"),
        )]
    })?;
    toml::from_str::<Theme>(&src).map_err(|e| {
        vec![Issue::error(
            "unparseable-toml",
            format!("failed to deserialize theme: {e}"),
        )]
    })
}

/// Validate a theme directory: reads `theme.toml`, runs `validate_manifest`,
/// then checks every non-empty `[sounds]` path resolves inside the theme
/// folder, exists, and is a permitted sound format (spec sections 2, 7).
pub fn validate_dir(theme_dir: &Path) -> Vec<Issue> {
    let mut issues = Vec::new();
    let manifest_path = theme_dir.join("theme.toml");
    let src = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            issues.push(Issue::error(
                "missing-manifest",
                format!("cannot read `theme.toml`: {e}"),
            ));
            return issues;
        }
    };
    issues.extend(validate_manifest(&src));

    let value: toml::Value = match toml::from_str(&src) {
        Ok(v) => v,
        Err(_) => return issues, // already reported by validate_manifest
    };
    let Some(sounds) = value.get("sounds").and_then(|v| v.as_table()) else {
        return issues;
    };

    let dir_canonical = match std::fs::canonicalize(theme_dir) {
        Ok(c) => c,
        Err(e) => {
            issues.push(Issue::error(
                "missing-asset",
                format!("cannot canonicalize theme dir: {e}"),
            ));
            return issues;
        }
    };

    for key in SOUNDS_KEYS {
        let Some(path_str) = sounds.get(*key).and_then(|v| v.as_str()) else {
            continue;
        };
        if path_str.is_empty() {
            continue;
        }

        // Reject absolute paths and any `..` component (spec section 2).
        // Leading `/` or `\` is rejected explicitly so the rule holds on
        // Windows where `Path::is_absolute` requires a drive letter.
        if Path::new(path_str).is_absolute()
            || path_str.starts_with('/')
            || path_str.starts_with('\\')
        {
            issues.push(Issue::error(
                "path-escape",
                format!("`sounds.{key}` is an absolute path: `{path_str}`"),
            ));
            continue;
        }
        if path_str.split(['/', '\\']).any(|c| c == "..") {
            issues.push(Issue::error(
                "path-escape",
                format!("`sounds.{key}` traverses outside the theme folder: `{path_str}`"),
            ));
            continue;
        }

        let resolved = theme_dir.join(path_str);
        let canonical = match std::fs::canonicalize(&resolved) {
            Ok(c) => c,
            Err(_) => {
                issues.push(Issue::error(
                    "missing-asset",
                    format!("`sounds.{key}` references missing file: `{path_str}`"),
                ));
                continue;
            }
        };
        // Symlink-escape guard (spec section 2): canonical path must stay
        // inside the canonical theme dir.
        if !canonical.starts_with(&dir_canonical) {
            issues.push(Issue::error(
                "path-escape",
                format!("`sounds.{key}` resolves outside the theme folder: `{path_str}`"),
            ));
            continue;
        }

        let ext = canonical
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext {
            Some(e) if SOUND_EXTS.contains(&e.as_str()) => {}
            _ => issues.push(Issue::error(
                "bad-asset-format",
                format!(
                    "`sounds.{key}` is not a permitted sound format (wav/ogg/flac/mp3): `{path_str}`"
                ),
            )),
        }
    }

    issues
}
