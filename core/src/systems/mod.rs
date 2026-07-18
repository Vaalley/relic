//! Platform registry (PLAN.md §4.4).
//!
//! Built-in definitions are TOML files under `core/data/systems/`, embedded at
//! compile time. Adding a platform is a data change, not a code change. Users
//! can override or extend these with files in their config directory (the
//! merge logic lands in Phase 1; the loader below already accepts external
//! TOML strings).

use serde::Deserialize;

use crate::{Error, Result};

const BUILTIN: &[&str] = &[
    include_str!("../../data/systems/nes.toml"),
    include_str!("../../data/systems/snes.toml"),
    include_str!("../../data/systems/gb.toml"),
    include_str!("../../data/systems/gba.toml"),
    include_str!("../../data/systems/megadrive.toml"),
    include_str!("../../data/systems/psx.toml"),
    include_str!("../../data/systems/n64.toml"),
    include_str!("../../data/systems/arcade.toml"),
    include_str!("../../data/systems/mastersystem.toml"),
    include_str!("../../data/systems/gamegear.toml"),
    include_str!("../../data/systems/pcengine.toml"),
    include_str!("../../data/systems/atari2600.toml"),
    include_str!("../../data/systems/nds.toml"),
    include_str!("../../data/systems/psp.toml"),
    include_str!("../../data/systems/saturn.toml"),
    include_str!("../../data/systems/dreamcast.toml"),
];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemDef {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub sort_order: i64,
    /// Lowercase extensions without dots, e.g. ["nes", "zip"].
    pub extensions: Vec<String>,
    /// RetroAchievements console id, if the platform is supported there.
    #[serde(default)]
    pub ra_console_id: Option<u32>,
    /// Suggested libretro core filename stem, e.g. "snes9x".
    #[serde(default)]
    pub default_core: Option<String>,
    /// Key theme authors use to pick per-system art.
    #[serde(default)]
    pub theme_key: Option<String>,
}

pub fn parse_system(toml_src: &str) -> Result<SystemDef> {
    let def: SystemDef = toml::from_str(toml_src).map_err(|e| Error::SystemDef {
        name: "<inline>".into(),
        reason: e.to_string(),
    })?;
    if def.extensions.is_empty() {
        return Err(Error::SystemDef {
            name: def.slug,
            reason: "at least one extension is required".into(),
        });
    }
    Ok(def)
}

/// All compiled-in platform definitions, sorted by `sort_order` then name.
pub fn builtin_systems() -> Result<Vec<SystemDef>> {
    let mut defs = BUILTIN
        .iter()
        .map(|src| parse_system(src))
        .collect::<Result<Vec<_>>>()?;
    defs.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.name.cmp(&b.name)));
    Ok(defs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse_and_are_unique() {
        let defs = builtin_systems().unwrap();
        assert!(defs.len() >= 8);
        let mut slugs: Vec<_> = defs.iter().map(|d| d.slug.as_str()).collect();
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), defs.len(), "duplicate system slug");
        for def in &defs {
            assert!(
                def.extensions
                    .iter()
                    .all(|e| e.to_lowercase() == *e && !e.starts_with('.')),
                "{}: extensions must be lowercase without dots",
                def.slug
            );
        }
    }
}
