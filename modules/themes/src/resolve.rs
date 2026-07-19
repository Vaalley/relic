//! Token resolution with per-key fallback (spec section 6).
//!
//! Phase 5 groundwork, PLAN.md section 6 layer 1. Resolution is
//! deterministic and never raises: a missing or broken theme degrades fully
//! to the built-in default theme. The missing-variant rule (spec 6.1) is
//! handled by per-key fallback — when a theme lacks the requested variant,
//! every color key falls through to the default theme's requested variant,
//! never the theme's other variant.

use crate::default_theme;
use crate::model::{ColorSet, Sounds, Theme};

/// Color variant selected by the viewer's theme preference (spec 6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Dark,
    Light,
}

/// Concrete, non-optional tokens handed to the shells. Every field is
/// populated; there are no `Option`s at this layer.
#[derive(Debug, Clone)]
pub struct ResolvedTokens {
    pub colors: ResolvedColors,
    pub font_family: String,
    pub scale: f64,
    pub radius: i64,
    pub sounds: ResolvedSounds,
}

#[derive(Debug, Clone)]
pub struct ResolvedColors {
    pub bg: String,
    pub surface: String,
    pub text: String,
    pub text_dim: String,
    pub accent: String,
    pub favorite: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedSounds {
    pub move_sound: String,
    pub select_sound: String,
    pub back_sound: String,
}

/// Resolve tokens for `variant`, falling back to the default theme per-key.
/// `theme = None` (or a broken theme the caller chose not to pass) resolves
/// fully to defaults.
pub fn resolve(theme: Option<&Theme>, variant: Variant) -> ResolvedTokens {
    let default = default_theme();

    let default_colors = default
        .colors
        .as_ref()
        .expect("bundled default theme must define [colors]");
    let default_variant: &ColorSet = match variant {
        Variant::Dark => default_colors.dark.as_ref(),
        Variant::Light => default_colors.light.as_ref(),
    }
    .expect("bundled default theme must define both variants");

    let theme_colors = theme.and_then(|t| t.colors.as_ref());
    // Missing-variant rule: if the theme lacks the requested variant, leave
    // `theme_variant` as `None` so every key falls through to the default
    // theme's requested variant — never the theme's other variant.
    let theme_variant: Option<&ColorSet> = match variant {
        Variant::Dark => theme_colors.and_then(|c| c.dark.as_ref()),
        Variant::Light => theme_colors.and_then(|c| c.light.as_ref()),
    };

    let colors = resolve_colors(theme_variant, default_variant);

    let font_family = theme
        .and_then(|t| t.typography.as_ref())
        .and_then(|t| t.font_family.clone())
        .or_else(|| {
            default
                .typography
                .as_ref()
                .and_then(|t| t.font_family.clone())
        })
        .unwrap_or_else(|| "Inter".to_string());

    let scale = theme
        .and_then(|t| t.typography.as_ref())
        .and_then(|t| t.scale)
        .or_else(|| default.typography.as_ref().and_then(|t| t.scale))
        .unwrap_or(1.0);

    let radius = theme
        .and_then(|t| t.shape.as_ref())
        .and_then(|s| s.radius)
        .or_else(|| default.shape.as_ref().and_then(|s| s.radius))
        .unwrap_or(8);

    let sounds = resolve_sounds(
        theme.and_then(|t| t.sounds.as_ref()),
        default.sounds.as_ref(),
    );

    ResolvedTokens {
        colors,
        font_family,
        scale,
        radius,
        sounds,
    }
}

fn resolve_colors(theme: Option<&ColorSet>, default: &ColorSet) -> ResolvedColors {
    ResolvedColors {
        bg: normalize_hex_color(&pick_str(
            theme.and_then(|c| c.bg.as_deref()),
            default.bg.as_deref(),
        )),
        surface: normalize_hex_color(&pick_str(
            theme.and_then(|c| c.surface.as_deref()),
            default.surface.as_deref(),
        )),
        text: normalize_hex_color(&pick_str(
            theme.and_then(|c| c.text.as_deref()),
            default.text.as_deref(),
        )),
        text_dim: normalize_hex_color(&pick_str(
            theme.and_then(|c| c.text_dim.as_deref()),
            default.text_dim.as_deref(),
        )),
        accent: normalize_hex_color(&pick_str(
            theme.and_then(|c| c.accent.as_deref()),
            default.accent.as_deref(),
        )),
        favorite: normalize_hex_color(&pick_str(
            theme.and_then(|c| c.favorite.as_deref()),
            default.favorite.as_deref(),
        )),
    }
}

/// Expand shorthand hex (`#rgb`/`#rgba`) to full form and drop any alpha
/// channel (`#rgba`/`#rrggbbaa` → `#rrggbb`). `validate.rs`'s `is_valid_hex`
/// accepts all four CSS-style forms per `docs/theme-format.md` §5.1, but
/// every current consumer (both shells' hex parsers) only understands
/// opaque `#rrggbb` — normalizing here means shells never need their own
/// shorthand/alpha handling. Anything not matching a valid hex shape passes
/// through unchanged (defensive; `is_valid_hex` is what actually rejects
/// bad input, at validation time, not here).
fn normalize_hex_color(s: &str) -> String {
    let Some(rest) = s.strip_prefix('#') else {
        return s.to_string();
    };
    if !rest.chars().all(|c| c.is_ascii_hexdigit()) {
        return s.to_string();
    }
    match rest.len() {
        3 => format!("#{}", rest.chars().flat_map(|c| [c, c]).collect::<String>()),
        4 => format!(
            "#{}",
            rest.chars()
                .take(3)
                .flat_map(|c| [c, c])
                .collect::<String>()
        ),
        8 => format!("#{}", &rest[..6]),
        _ => s.to_string(),
    }
}

fn resolve_sounds(theme: Option<&Sounds>, default: Option<&Sounds>) -> ResolvedSounds {
    ResolvedSounds {
        move_sound: pick_str(
            theme.and_then(|s| s.r#move.as_deref()),
            default.and_then(|s| s.r#move.as_deref()),
        ),
        select_sound: pick_str(
            theme.and_then(|s| s.select.as_deref()),
            default.and_then(|s| s.select.as_deref()),
        ),
        back_sound: pick_str(
            theme.and_then(|s| s.back.as_deref()),
            default.and_then(|s| s.back.as_deref()),
        ),
    }
}

/// Per-key fallback: take the theme's value, else the default's, else `""`.
/// The final `""` arm only triggers if both sides are absent, which for the
/// bundled default never happens — it is a defensive floor so resolution can
/// never panic (spec 6: "deterministic and never raises").
fn pick_str(theme: Option<&str>, default: Option<&str>) -> String {
    theme.or(default).unwrap_or("").to_string()
}
