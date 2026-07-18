//! relic-themes — the theme engine (PLAN.md section 6).
//!
//! Two layers, shipped in order:
//! - Layer 1 (1.0): design tokens — colors, typography, shape, sounds —
//!   loaded from a theme manifest (`theme.toml` + assets); see
//!   `themes/default/theme.toml` for the shape of a token theme.
//! - Layer 2 (post-1.0): declarative, data-bound screen layouts.
//!
//! Themes are pure data + assets: no network access and no filesystem
//! access outside their own folder. A broken theme degrades to the default
//! theme with a visible warning, never a crash.
//!
//! Phase 5 groundwork, PLAN.md section 6 layer 1: this crate ships the
//! loader (`parse_theme`), validator (`validate_manifest`, `validate_dir`),
//! and resolver (`resolve`) for layer-1 tokens. The manifest format is
//! specified in `docs/theme-format.md`.

mod model;
mod resolve;
mod validate;

pub use model::Theme;
pub use resolve::{ResolvedColors, ResolvedSounds, ResolvedTokens, Variant};
pub use validate::{Issue, Severity};

/// Bundled default theme, the canonical fallback (spec section 6). Embedded
/// at compile time from `themes/default/theme.toml`.
const DEFAULT_TOML: &str = include_str!("../../../themes/default/theme.toml");

/// Stable identifier this module reports through the engine's
/// `capabilities()` API so shells can hide UI for absent modules.
pub fn capability_id() -> &'static str {
    "themes"
}

/// The built-in default theme, parsed once and cached for the process
/// lifetime. Used as the per-key fallback base for every resolution.
pub fn default_theme() -> &'static Theme {
    static DEFAULT: std::sync::OnceLock<Theme> = std::sync::OnceLock::new();
    DEFAULT.get_or_init(|| {
        parse_theme(DEFAULT_TOML).expect("bundled default theme must parse with no errors")
    })
}

/// Parse a manifest into a typed `Theme`. Returns `Err(issues)` iff any
/// error-severity issue is present; otherwise `Ok(theme)`. Warnings are
/// not returned here — call `validate_manifest` to retrieve them.
pub fn parse_theme(toml_src: &str) -> Result<Theme, Vec<Issue>> {
    validate::parse_theme(toml_src)
}

/// Validate a manifest string. Returns every issue (errors and warnings).
/// Pure: reads no files, makes no network calls.
pub fn validate_manifest(toml_src: &str) -> Vec<Issue> {
    validate::validate_manifest(toml_src)
}

/// Validate a theme directory: reads `theme.toml`, runs `validate_manifest`,
/// then checks referenced `[sounds]` assets resolve inside the theme
/// folder, exist, and are a permitted format (spec sections 2, 7).
pub fn validate_dir(theme_dir: &std::path::Path) -> Vec<Issue> {
    validate::validate_dir(theme_dir)
}

/// Resolve concrete tokens for `variant`, falling back per-key to the
/// default theme. `theme = None` resolves fully to defaults (spec 6.4).
pub fn resolve(theme: Option<&Theme>, variant: Variant) -> ResolvedTokens {
    resolve::resolve(theme, variant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_id_is_themes() {
        assert_eq!(capability_id(), "themes");
    }

    #[test]
    fn default_theme_parses_with_zero_issues() {
        let issues = validate_manifest(DEFAULT_TOML);
        assert!(
            issues.is_empty(),
            "bundled default theme must produce no issues, got: {issues:?}"
        );
        let theme = parse_theme(DEFAULT_TOML).expect("default theme parses");
        assert_eq!(theme.theme.name.as_deref(), Some("Default"));
        assert_eq!(theme.theme.format_version, Some(1));
    }

    #[test]
    fn recolor_only_theme_resolves_with_defaults_filling_gaps() {
        let src = r##"
[theme]
name = "Recolor"
format_version = 1

[colors.dark]
accent = "#ff0000"
"##;
        let theme = parse_theme(src).expect("recolor theme parses cleanly");
        let tokens = resolve(Some(&theme), Variant::Dark);
        // The one overridden key.
        assert_eq!(tokens.colors.accent, "#ff0000");
        // Everything else comes from the default dark variant.
        assert_eq!(tokens.colors.bg, "#121212");
        assert_eq!(tokens.colors.surface, "#1e1e1e");
        assert_eq!(tokens.colors.text, "#f2f2f2");
        assert_eq!(tokens.colors.text_dim, "#a0a0a0");
        assert_eq!(tokens.colors.favorite, "#ffcf5c");
        // Non-color tokens are entirely default.
        assert_eq!(tokens.font_family, "Inter");
        assert_eq!(tokens.scale, 1.0);
        assert_eq!(tokens.radius, 8);
        assert_eq!(tokens.sounds.move_sound, "");
        assert_eq!(tokens.sounds.select_sound, "");
        assert_eq!(tokens.sounds.back_sound, "");
    }

    #[test]
    fn missing_variant_falls_back_to_default_requested_variant() {
        // Theme ships only `dark`; requesting `light` must use the DEFAULT
        // theme's light variant, never the theme's dark variant (spec 6.1).
        let src = r##"
[theme]
name = "DarkOnly"
format_version = 1

[colors.dark]
accent = "#ff0000"
bg = "#000000"
"##;
        let theme = parse_theme(src).expect("parses");
        let tokens = resolve(Some(&theme), Variant::Light);
        // Default light accent is #3a5fd9, not the theme's dark #ff0000.
        assert_eq!(tokens.colors.accent, "#3a5fd9");
        // Default light bg is #f7f7f7, not the theme's dark #000000.
        assert_eq!(tokens.colors.bg, "#f7f7f7");
    }

    #[test]
    fn none_theme_resolves_fully_to_defaults() {
        let dark = resolve(None, Variant::Dark);
        assert_eq!(dark.colors.bg, "#121212");
        assert_eq!(dark.colors.accent, "#7c9eff");
        assert_eq!(dark.font_family, "Inter");
        assert_eq!(dark.scale, 1.0);
        assert_eq!(dark.radius, 8);

        let light = resolve(None, Variant::Light);
        assert_eq!(light.colors.bg, "#f7f7f7");
        assert_eq!(light.colors.accent, "#3a5fd9");
    }

    fn assert_has_error(issues: &[Issue], code: &str) {
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.code == code),
            "expected an Error with code `{code}`, got: {issues:?}"
        );
    }

    fn assert_has_warning(issues: &[Issue], code: &str) {
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Warning && i.code == code),
            "expected a Warning with code `{code}`, got: {issues:?}"
        );
    }

    #[test]
    fn missing_format_version_is_error() {
        let src = r##"
[theme]
name = "X"
"##;
        assert_has_error(&validate_manifest(src), "missing-format-version");
    }

    #[test]
    fn missing_name_is_error() {
        let src = r##"
[theme]
format_version = 1
"##;
        assert_has_error(&validate_manifest(src), "missing-name");
    }

    #[test]
    fn format_version_0_and_2_are_rejected() {
        for fv in [0i64, 2] {
            let src = format!("[theme]\nname = \"X\"\nformat_version = {fv}\n");
            let issues = validate_manifest(&src);
            assert_has_error(&issues, "unsupported-format-version");
            assert!(parse_theme(&src).is_err(), "fv={fv} must not parse");
        }
    }

    #[test]
    fn bad_color_is_error() {
        let src = r##"
[theme]
name = "X"
format_version = 1

[colors.dark]
accent = "not-a-color"
"##;
        assert_has_error(&validate_manifest(src), "bad-color");
    }

    #[test]
    fn bad_color_short_hex_rejected() {
        // 5-char hex (#RRGGB without alpha) is not in the allowed set.
        let src = r##"
[theme]
name = "X"
format_version = 1

[colors.dark]
bg = "#12345"
"##;
        assert_has_error(&validate_manifest(src), "bad-color");
    }

    #[test]
    fn valid_hex_forms_accepted() {
        let src = r##"
[theme]
name = "X"
format_version = 1

[colors.dark]
bg = "#abc"
surface = "#abcd"
text = "#aabbcc"
text_dim = "#aabbccdd"
accent = "#FFFFFF"
favorite = "#FFAaBB"
"##;
        let issues = validate_manifest(src);
        assert!(
            !issues.iter().any(|i| i.code == "bad-color"),
            "all valid hex forms should be accepted, got: {issues:?}"
        );
    }

    #[test]
    fn scale_le_zero_is_error() {
        let src = r##"
[theme]
name = "X"
format_version = 1

[typography]
scale = 0.0
"##;
        assert_has_error(&validate_manifest(src), "scale-out-of-range");
    }

    #[test]
    fn scale_outside_range_is_warning_only() {
        let src = r##"
[theme]
name = "X"
format_version = 1

[typography]
scale = 4.0
"##;
        let issues = validate_manifest(src);
        assert_has_warning(&issues, "scale-out-of-range");
        assert!(
            !issues.iter().any(|i| i.severity == Severity::Error),
            "out-of-range scale must not be an error, got: {issues:?}"
        );
        parse_theme(src).expect("warnings do not block parsing");
    }

    #[test]
    fn radius_negative_is_error() {
        let src = r##"
[theme]
name = "X"
format_version = 1

[shape]
radius = -1
"##;
        assert_has_error(&validate_manifest(src), "radius-negative");
    }

    #[test]
    fn unknown_key_is_warning_only() {
        let src = r##"
[theme]
name = "X"
format_version = 1
bogus = 5

[colors.dark]
bogus = "#000000"

[shape]
bogus = 1

[sounds]
bogus = "x.wav"
"##;
        let issues = validate_manifest(src);
        assert_has_warning(&issues, "unknown-key");
        assert!(
            !issues.iter().any(|i| i.severity == Severity::Error),
            "unknown keys must not be errors, got: {issues:?}"
        );
    }

    #[test]
    fn parse_theme_returns_err_when_error_present() {
        let src = "[theme]\nname = \"X\"\n";
        assert!(parse_theme(src).is_err());
    }

    #[test]
    fn unparseable_toml_is_error() {
        let src = "this is not = = valid toml";
        let issues = validate_manifest(src);
        assert_has_error(&issues, "unparseable-toml");
    }

    #[test]
    fn validate_dir_catches_escape_paths_and_missing_files() {
        use std::fs;
        let dir = tempfile::tempdir().expect("tempdir created");
        let root = dir.path();

        // `move` is an absolute path; `select` uses `..` traversal; `back`
        // points at a non-existent file inside the folder.
        let src = r##"
[theme]
name = "Dir"
format_version = 1

[sounds]
move = "/etc/passwd"
select = "../escape.wav"
back = "missing.wav"
"##;
        fs::write(root.join("theme.toml"), src).expect("write manifest");

        let issues = validate_dir(root);
        let escapes: Vec<&Issue> = issues
            .iter()
            .filter(|i| i.code == "path-escape" && i.severity == Severity::Error)
            .collect();
        assert!(
            escapes.len() >= 2,
            "expected at least two path-escape errors (absolute + `..`), got: {issues:?}"
        );
        assert_has_error(&issues, "missing-asset");
    }

    #[test]
    fn validate_dir_catches_bad_sound_extension() {
        use std::fs;
        let dir = tempfile::tempdir().expect("tempdir created");
        let root = dir.path();
        fs::write(
            root.join("theme.toml"),
            r##"
[theme]
name = "Dir"
format_version = 1

[sounds]
move = "beep.txt"
"##,
        )
        .expect("write manifest");
        fs::write(root.join("beep.txt"), b"data").expect("write asset");

        let issues = validate_dir(root);
        assert_has_error(&issues, "bad-asset-format");
    }

    #[test]
    fn validate_dir_accepts_valid_sound() {
        use std::fs;
        let dir = tempfile::tempdir().expect("tempdir created");
        let root = dir.path();
        fs::write(
            root.join("theme.toml"),
            r##"
[theme]
name = "Dir"
format_version = 1

[sounds]
move = "sounds/move.wav"
"##,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("sounds")).expect("mkdir");
        fs::write(root.join("sounds").join("move.wav"), b"RIFF").expect("write asset");

        let issues = validate_dir(root);
        assert!(
            issues.is_empty(),
            "valid sound reference should produce no issues, got: {issues:?}"
        );
    }

    #[test]
    fn validate_dir_missing_manifest_is_error() {
        let dir = tempfile::tempdir().expect("tempdir created");
        let issues = validate_dir(dir.path());
        assert_has_error(&issues, "missing-manifest");
    }
}
