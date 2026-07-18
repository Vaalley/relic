//! Serde model for the layer-1 theme manifest (spec sections 4-5).
//!
//! Phase 5 groundwork, PLAN.md section 6 layer 1. Every token field is
//! optional at the manifest level; resolution (see `resolve`) fills gaps
//! from the built-in default theme, so a theme may ship any subset.

use serde::{Deserialize, Deserializer};

/// Deserialize an optional TOML number (int or float) into `Option<f64>`.
/// TOML keeps integer and float literals as distinct value kinds; the spec
/// (section 5.2) calls `scale` a float, but a bare `1` is a reasonable author
/// mistake and `validate_manifest` already accepts any number, so the model
/// must not reject what the validator accepts.
fn number_to_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    let v = Option::<toml::Value>::deserialize(d)?;
    Ok(v.and_then(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64))))
}

/// Root manifest. All top-level tables are optional; missing ones fall back
/// to the default theme during resolution (spec section 6).
#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    #[serde(default)]
    pub theme: ThemeMeta,
    #[serde(default)]
    pub colors: Option<Colors>,
    #[serde(default)]
    pub typography: Option<Typography>,
    #[serde(default)]
    pub shape: Option<Shape>,
    #[serde(default)]
    pub sounds: Option<Sounds>,
}

/// `[theme]` metadata. `name` and `format_version` are required by the spec;
/// they are modelled as `Option` here so the validator can report a precise
/// issue rather than a generic deserialize error.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ThemeMeta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub format_version: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
}

/// `[colors]` holds no keys of its own, only the `dark` and `light`
/// subtables (spec section 5.1).
#[derive(Debug, Clone, Deserialize)]
pub struct Colors {
    #[serde(default)]
    pub dark: Option<ColorSet>,
    #[serde(default)]
    pub light: Option<ColorSet>,
}

/// A single color variant. Values are raw hex strings; validity is checked
/// by the validator, not the type system.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ColorSet {
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub surface: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub text_dim: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub favorite: Option<String>,
}

/// `[typography]` (spec section 5.2).
#[derive(Debug, Clone, Deserialize)]
pub struct Typography {
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default, deserialize_with = "number_to_f64")]
    pub scale: Option<f64>,
}

/// `[shape]` (spec section 5.3).
#[derive(Debug, Clone, Deserialize)]
pub struct Shape {
    #[serde(default)]
    pub radius: Option<i64>,
}

/// `[sounds]` (spec section 5.4). `move` is a Rust keyword, so the raw
/// identifier form is used; serde serializes it as the TOML key `move`.
#[derive(Debug, Clone, Deserialize)]
pub struct Sounds {
    #[serde(default)]
    pub r#move: Option<String>,
    #[serde(default)]
    pub select: Option<String>,
    #[serde(default)]
    pub back: Option<String>,
}
