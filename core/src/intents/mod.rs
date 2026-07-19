//! Android intent-template registry and validator (PLAN.md §4.5,
//! docs/android-intents.md).
//!
//! Built-in templates are TOML files under `core/data/intents/`, embedded at
//! compile time — the same pattern as [`crate::systems`]. This module is
//! pure parsing + validation (docs/android-intents.md §6); it does not build
//! or fire an Android `Intent` (that's the shell's job once the Phase 3
//! resolver lands — see `apps/android/README.md` "Alpha shortcuts").
//! `relic-cli intent validate` is the primary consumer today.

use std::collections::HashMap;

use serde::Deserialize;

use crate::{Error, Result};

const BUILTIN: &[(&str, &str)] = &[
    (
        "retroarch_aarch64",
        include_str!("../../data/intents/retroarch_aarch64.toml"),
    ),
    (
        "retroarch",
        include_str!("../../data/intents/retroarch.toml"),
    ),
    ("ppsspp", include_str!("../../data/intents/ppsspp.toml")),
    (
        "ppsspp_gold",
        include_str!("../../data/intents/ppsspp_gold.toml"),
    ),
    (
        "ppsspp_legacy",
        include_str!("../../data/intents/ppsspp_legacy.toml"),
    ),
    ("dolphin", include_str!("../../data/intents/dolphin.toml")),
    (
        "dolphin_mmjr",
        include_str!("../../data/intents/dolphin_mmjr.toml"),
    ),
    ("melonds", include_str!("../../data/intents/melonds.toml")),
    (
        "duckstation",
        include_str!("../../data/intents/duckstation.toml"),
    ),
    (
        "aethersx2",
        include_str!("../../data/intents/aethersx2.toml"),
    ),
    ("azahar", include_str!("../../data/intents/azahar.toml")),
    (
        "mupen64plus_fz",
        include_str!("../../data/intents/mupen64plus_fz.toml"),
    ),
    (
        "yabasanshiro2",
        include_str!("../../data/intents/yabasanshiro2.toml"),
    ),
];

/// Placeholders recognized in `[[extras]]` values (docs/android-intents.md §4.3).
const KNOWN_PLACEHOLDERS: &[&str] = &["rom_uri", "rom_path", "core"];

/// `Intent.FLAG_*` names templates may reference (docs/android-intents.md §4.5).
const KNOWN_FLAGS: &[&str] = &[
    "FLAG_GRANT_READ_URI_PERMISSION",
    "FLAG_ACTIVITY_NEW_TASK",
    "FLAG_ACTIVITY_CLEAR_TOP",
    "FLAG_ACTIVITY_SINGLE_TOP",
    "FLAG_ACTIVITY_NO_HISTORY",
    "FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS",
];

/// Forbidden by the security model (docs/android-intents.md §5).
const FORBIDDEN_FLAG: &str = "FLAG_GRANT_WRITE_URI_PERMISSION";

/// Template ids allowed to use the `{core}` placeholder (docs/android-intents.md
/// §4.3): libretro frontends only. Two RetroArch package aliases are shipped
/// (stable + AArch64 nightly) but share one libretro interface.
const LIBRETRO_FRONTEND_IDS: &[&str] = &["retroarch", "retroarch_aarch64"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataMode {
    Data,
    Extra,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtraType {
    String,
    Bool,
    Int,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extra {
    pub name: String,
    #[serde(rename = "type")]
    pub extra_type: ExtraType,
    pub value: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerSystemOverride {
    pub activity: Option<String>,
    pub action: Option<String>,
    pub data_mode: Option<DataMode>,
    pub data_extra_name: Option<String>,
    pub data_mime_type: Option<String>,
    #[serde(default)]
    pub extras: Vec<Extra>,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentTemplate {
    pub id: String,
    pub display_name: String,
    pub package: String,
    pub activity: String,
    pub action: String,
    pub data_mode: DataMode,
    #[serde(default)]
    pub data_extra_name: Option<String>,
    #[serde(default)]
    pub data_mime_type: Option<String>,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub min_version_code: Option<u32>,
    #[serde(default)]
    pub extras: Vec<Extra>,
    #[serde(default)]
    pub per_system: HashMap<String, PerSystemOverride>,
    /// System slugs this template targets, or `["*"]` for every registry
    /// system (docs/android-intents.md §4.1). Lets a shell pick candidate
    /// templates for a game's system without any per-emulator code.
    pub systems: Vec<String>,
}

/// Parse a template from its TOML source. Does not validate (§6) — call
/// [`validate`] separately, since validation needs the filename stem and the
/// set of known system slugs, neither of which this function has.
pub fn parse_intent(toml_src: &str) -> Result<IntentTemplate> {
    toml::from_str(toml_src).map_err(|e| Error::IntentDef {
        id: "<inline>".into(),
        reason: e.to_string(),
    })
}

/// Validate a parsed template against docs/android-intents.md §6.
/// `filename_stem` is the template file's stem (must equal `id`).
/// `known_slugs` is the set of valid `per_system` keys (system registry
/// slugs). Returns every violation found, not just the first.
pub fn validate(
    template: &IntentTemplate,
    filename_stem: &str,
    known_slugs: &[String],
) -> Vec<String> {
    let mut errors = Vec::new();

    // 1. id equals filename stem.
    if template.id != filename_stem {
        errors.push(format!(
            "id '{}' does not match filename stem '{}'",
            template.id, filename_stem
        ));
    }

    // 2. package / activity non-empty, dotted, no spaces.
    for (field, value) in [
        ("package", &template.package),
        ("activity", &template.activity),
    ] {
        if value.is_empty() || !value.contains('.') || value.contains(' ') {
            errors.push(format!(
                "{field} '{value}' must be a non-empty, dotted, space-free component name"
            ));
        }
    }

    // 3. action is a known action string shape.
    if !template.action.starts_with("android.intent.action.") {
        errors.push(format!(
            "action '{}' must start with 'android.intent.action.'",
            template.action
        ));
    }

    // 4. data_mode / data_extra_name pairing; "none" forbids {rom_uri} in extras.
    match template.data_mode {
        DataMode::Extra => {
            match template.data_extra_name.as_deref() {
                None | Some("") => {
                    errors
                        .push("data_mode = \"extra\" requires a non-empty data_extra_name".into());
                }
                Some(name) => {
                    // §5 step 5/6: the shell carries the ROM URI via the extra
                    // named data_extra_name. Every shipped template makes this
                    // concrete by listing that extra in [[extras]] with
                    // value = "{rom_uri}" — catch the case where a template
                    // declares data_extra_name but forgets to actually wire
                    // the ROM into extras (or points it at the wrong extra).
                    let carries_rom = template
                        .extras
                        .iter()
                        .any(|e| e.name == name && e.value == "{rom_uri}");
                    if !carries_rom {
                        errors.push(format!(
                            "data_mode = \"extra\" names data_extra_name '{name}', but no \
                             [[extras]] entry has name = '{name}' and value = \"{{rom_uri}}\""
                        ));
                    }
                }
            }
        }
        DataMode::None => {
            for extra in &template.extras {
                if extra.value.contains("{rom_uri}") {
                    errors.push(format!(
                        "data_mode = \"none\" but extra '{}' references {{rom_uri}}",
                        extra.name
                    ));
                }
            }
        }
        _ => {}
    }

    // 5 + 7. extras: name/type/value well-formed, placeholders known.
    for extra in &template.extras {
        if extra.name.is_empty() {
            errors.push("an extra has an empty name".into());
        }
        match extra.extra_type {
            ExtraType::Bool if extra.value != "true" && extra.value != "false" => {
                errors.push(format!(
                    "extra '{}' has type = \"bool\" but value '{}' is not \"true\"/\"false\"",
                    extra.name, extra.value
                ));
            }
            ExtraType::Int if extra.value.parse::<i64>().is_err() => {
                errors.push(format!(
                    "extra '{}' has type = \"int\" but value '{}' is not a base-10 integer",
                    extra.name, extra.value
                ));
            }
            _ => {}
        }
        for placeholder in find_placeholders(&extra.value) {
            if !KNOWN_PLACEHOLDERS.contains(&placeholder.as_str()) {
                errors.push(format!(
                    "extra '{}' references unknown placeholder {{{placeholder}}}",
                    extra.name
                ));
            }
            // 6. {core} is libretro-frontend-only.
            if placeholder == "core" && !LIBRETRO_FRONTEND_IDS.contains(&template.id.as_str()) {
                errors.push(format!(
                    "extra '{}' references {{core}}, but {{core}} is only valid in a \
                     libretro-frontend template ({LIBRETRO_FRONTEND_IDS:?})",
                    extra.name
                ));
            }
        }
    }

    // 8. flags: known names, write-grant forbidden.
    for flag in &template.flags {
        if flag == FORBIDDEN_FLAG {
            errors.push(format!(
                "flag '{flag}' is forbidden (read-only security model)"
            ));
        } else if !KNOWN_FLAGS.contains(&flag.as_str()) {
            errors.push(format!("flag '{flag}' is not a known Intent.FLAG_* name"));
        }
    }

    // 9. per_system keys are known system slugs; validate their extras/flags too.
    for (slug, over) in &template.per_system {
        if !known_slugs.iter().any(|s| s == slug) {
            errors.push(format!(
                "per_system key '{slug}' is not a known system slug"
            ));
        }
        for extra in &over.extras {
            for placeholder in find_placeholders(&extra.value) {
                if !KNOWN_PLACEHOLDERS.contains(&placeholder.as_str()) {
                    errors.push(format!(
                        "per_system.{slug} extra '{}' references unknown placeholder {{{placeholder}}}",
                        extra.name
                    ));
                }
            }
        }
        for flag in &over.flags {
            if flag == FORBIDDEN_FLAG {
                errors.push(format!(
                    "per_system.{slug} flag '{flag}' is forbidden (read-only security model)"
                ));
            } else if !KNOWN_FLAGS.contains(&flag.as_str()) {
                errors.push(format!(
                    "per_system.{slug} flag '{flag}' is not a known Intent.FLAG_* name"
                ));
            }
        }
    }

    // 11. systems: non-empty, either exactly ["*"] or known slugs (no mixing).
    if template.systems.is_empty() {
        errors.push("systems must not be empty".into());
    } else if template.systems.iter().any(|s| s == "*") {
        if template.systems.len() > 1 {
            errors.push("systems: '*' must be the only entry, not mixed with slugs".into());
        }
    } else {
        for slug in &template.systems {
            if !known_slugs.iter().any(|s| s == slug) {
                errors.push(format!("systems entry '{slug}' is not a known system slug"));
            }
        }
    }

    errors
}

/// The system slugs a template applies to, for shells picking candidate
/// templates for a game's system. `known_slugs` is the full registry, used to
/// expand a `["*"]` wildcard (docs/android-intents.md §4.1).
pub fn applies_to(template: &IntentTemplate, system_slug: &str) -> bool {
    template
        .systems
        .iter()
        .any(|s| s == "*" || s == system_slug)
}

/// One resolved `[[extras]]` entry, ready for the shell to call the matching
/// `putExtra` overload with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedExtra {
    pub name: String,
    pub extra_type: ExtraType,
    pub value: String,
}

/// A template fully merged with its `per_system` override (if any) and with
/// every placeholder substituted — everything the shell needs to build and
/// fire one explicit `Intent` (docs/android-intents.md §5). `flags` always
/// includes `FLAG_GRANT_READ_URI_PERMISSION` and `FLAG_ACTIVITY_NEW_TASK`
/// (added implicitly per §5 step 7 if the template didn't list them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedIntent {
    pub package: String,
    pub activity: String,
    pub action: String,
    pub data_mode: DataMode,
    pub data_extra_name: Option<String>,
    pub data_mime_type: Option<String>,
    pub extras: Vec<ResolvedExtra>,
    pub flags: Vec<String>,
}

/// Placeholder values available at launch time (docs/android-intents.md
/// §4.3). `core` must already be the full path the shell wants substituted
/// (e.g. RetroArch's `/data/data/<pkg>/cores/<stem>_libretro_android.so`) —
/// this module has no notion of Android package layout, only string
/// substitution.
pub struct LaunchContext<'a> {
    pub rom_uri: &'a str,
    pub rom_path: &'a str,
    pub core: Option<&'a str>,
}

/// Merge `template`'s `per_system.<system_slug>` override (if present) over
/// its top-level fields, then substitute placeholders in every extra value.
/// Assumes `template` already passed [`validate`] — this does not re-check
/// unknown placeholders or `{core}`-outside-RetroArch, it just substitutes.
pub fn resolve(
    template: &IntentTemplate,
    system_slug: &str,
    ctx: &LaunchContext<'_>,
) -> ResolvedIntent {
    let over = template.per_system.get(system_slug);

    let activity = over
        .and_then(|o| o.activity.clone())
        .unwrap_or_else(|| template.activity.clone());
    let action = over
        .and_then(|o| o.action.clone())
        .unwrap_or_else(|| template.action.clone());
    let data_mode = over.and_then(|o| o.data_mode).unwrap_or(template.data_mode);
    let data_extra_name = over
        .and_then(|o| o.data_extra_name.clone())
        .or_else(|| template.data_extra_name.clone());
    let data_mime_type = over
        .and_then(|o| o.data_mime_type.clone())
        .or_else(|| template.data_mime_type.clone());

    // §4.4: extras/flags replace wholesale when the override supplies any.
    let extras_src: &[Extra] = match over {
        Some(o) if !o.extras.is_empty() => &o.extras,
        _ => &template.extras,
    };
    let flags_src: &[String] = match over {
        Some(o) if !o.flags.is_empty() => &o.flags,
        _ => &template.flags,
    };

    let extras = extras_src
        .iter()
        .map(|e| ResolvedExtra {
            name: e.name.clone(),
            extra_type: e.extra_type,
            value: substitute(&e.value, ctx),
        })
        .collect();

    let mut flags: Vec<String> = flags_src.to_vec();
    if !flags.iter().any(|f| f == "FLAG_GRANT_READ_URI_PERMISSION") {
        flags.push("FLAG_GRANT_READ_URI_PERMISSION".to_string());
    }
    if !flags.iter().any(|f| f == "FLAG_ACTIVITY_NEW_TASK") {
        flags.push("FLAG_ACTIVITY_NEW_TASK".to_string());
    }

    ResolvedIntent {
        package: template.package.clone(),
        activity,
        action,
        data_mode,
        data_extra_name,
        data_mime_type,
        extras,
        flags,
    }
}

/// Substitute `{rom_uri}`/`{rom_path}`/`{core}` in a `value` string, treating
/// `{{`/`}}` as literal braces (docs/android-intents.md §4.3). Any other
/// `{name}` is left as-is — [`validate`] is what rejects unknown
/// placeholders before a template ever reaches this function.
fn substitute(value: &str, ctx: &LaunchContext<'_>) -> String {
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < value.len() {
        let rest = &value[i..];
        if let Some(stripped) = rest.strip_prefix("{{") {
            out.push('{');
            i = value.len() - stripped.len();
        } else if let Some(stripped) = rest.strip_prefix("}}") {
            out.push('}');
            i = value.len() - stripped.len();
        } else if let Some(after) = rest.strip_prefix('{') {
            match after.find('}') {
                Some(end) => {
                    let name = &after[..end];
                    match name {
                        "rom_uri" => out.push_str(ctx.rom_uri),
                        "rom_path" => out.push_str(ctx.rom_path),
                        "core" => out.push_str(ctx.core.unwrap_or_default()),
                        _ => {
                            out.push('{');
                            out.push_str(name);
                            out.push('}');
                        }
                    }
                    i = value.len() - after.len() + end + 1;
                }
                None => {
                    out.push('{');
                    i += 1;
                }
            }
        } else {
            let ch = rest.chars().next().expect("i < value.len()");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn find_placeholders(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find('{') {
        // `{{`/`}}` are the literal-brace escape (docs/android-intents.md §4.3), not
        // a placeholder open.
        if rest[start + 1..].starts_with('{') {
            rest = &rest[start + 2..];
            continue;
        }
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                out.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out
}

/// All compiled-in intent templates, parsed and with their filename stems.
/// Does not validate; pair with [`validate`] for a full check.
pub fn builtin_intents() -> Result<Vec<(String, IntentTemplate)>> {
    BUILTIN
        .iter()
        .map(|(stem, src)| {
            let template = toml::from_str::<IntentTemplate>(src).map_err(|e| Error::IntentDef {
                id: (*stem).to_string(),
                reason: e.to_string(),
            })?;
            Ok(((*stem).to_string(), template))
        })
        .collect()
}

/// The raw (filename stem, TOML source) pairs behind [`builtin_intents`], for
/// callers that need to re-report validation errors against the original
/// source (e.g. `relic-cli intent validate`).
pub fn builtin_sources() -> &'static [(&'static str, &'static str)] {
    BUILTIN
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::builtin_systems;

    fn slugs() -> Vec<String> {
        builtin_systems()
            .unwrap()
            .into_iter()
            .map(|s| s.slug)
            .collect()
    }

    #[test]
    fn all_builtin_templates_parse() {
        let intents = builtin_intents().unwrap();
        assert!(intents.len() >= 12);
    }

    #[test]
    fn all_builtin_templates_pass_validation() {
        let slugs = slugs();
        for (stem, template) in builtin_intents().unwrap() {
            let errors = validate(&template, &stem, &slugs);
            assert!(
                errors.is_empty(),
                "template '{stem}' failed validation: {errors:?}"
            );
        }
    }

    #[test]
    fn mismatched_id_is_rejected() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.VIEW"
            data_mode = "data"
            systems = ["snes"]
            "#,
        )
        .unwrap();
        let errors = validate(&t, "bar", &[]);
        assert!(errors.iter().any(|e| e.contains("does not match")));
    }

    #[test]
    fn core_placeholder_outside_retroarch_is_rejected() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.MAIN"
            data_mode = "extra"
            data_extra_name = "ROM"
            systems = ["snes"]

            [[extras]]
            name = "ROM"
            type = "string"
            value = "{core}"
            "#,
        )
        .unwrap();
        let errors = validate(&t, "foo", &[]);
        assert!(errors.iter().any(|e| e.contains("{core}")));
    }

    #[test]
    fn write_permission_flag_is_forbidden() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.VIEW"
            data_mode = "data"
            flags = ["FLAG_GRANT_WRITE_URI_PERMISSION"]
            systems = ["snes"]
            "#,
        )
        .unwrap();
        let errors = validate(&t, "foo", &[]);
        assert!(errors.iter().any(|e| e.contains("forbidden")));
    }

    #[test]
    fn unknown_placeholder_is_rejected() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.MAIN"
            data_mode = "extra"
            data_extra_name = "ROM"
            systems = ["snes"]

            [[extras]]
            name = "ROM"
            type = "string"
            value = "{bogus}"
            "#,
        )
        .unwrap();
        let errors = validate(&t, "foo", &[]);
        assert!(errors.iter().any(|e| e.contains("unknown placeholder")));
    }

    #[test]
    fn bool_extra_requires_true_or_false() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.MAIN"
            data_mode = "extra"
            data_extra_name = "ROM"
            systems = ["snes"]

            [[extras]]
            name = "FLAG"
            type = "bool"
            value = "yes"
            "#,
        )
        .unwrap();
        let errors = validate(&t, "foo", &[]);
        assert!(errors.iter().any(|e| e.contains("\"true\"/\"false\"")));
    }

    #[test]
    fn extra_mode_without_rom_carrying_extra_is_rejected() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.MAIN"
            data_mode = "extra"
            data_extra_name = "ROM"
            systems = ["snes"]

            [[extras]]
            name = "SOMETHING_ELSE"
            type = "string"
            value = "fixed"
            "#,
        )
        .unwrap();
        let errors = validate(&t, "foo", &[]);
        assert!(errors.iter().any(|e| e.contains("no [[extras]] entry")));
    }

    #[test]
    fn unknown_per_system_slug_is_rejected() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.VIEW"
            data_mode = "data"
            systems = ["snes"]

            [per_system.not_a_real_slug]
            action = "android.intent.action.MAIN"
            "#,
        )
        .unwrap();
        let errors = validate(&t, "foo", &["snes".to_string()]);
        assert!(errors.iter().any(|e| e.contains("not a known system slug")));
    }

    #[test]
    fn literal_braces_are_not_placeholders() {
        assert_eq!(find_placeholders("{{literal}}"), Vec::<String>::new());
        assert_eq!(find_placeholders("{rom_uri}"), vec!["rom_uri".to_string()]);
    }

    #[test]
    fn empty_systems_is_rejected() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.VIEW"
            data_mode = "data"
            systems = []
            "#,
        )
        .unwrap();
        let errors = validate(&t, "foo", &["snes".to_string()]);
        assert!(errors
            .iter()
            .any(|e| e.contains("systems must not be empty")));
    }

    #[test]
    fn unknown_systems_entry_is_rejected() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.VIEW"
            data_mode = "data"
            systems = ["not_a_real_slug"]
            "#,
        )
        .unwrap();
        let errors = validate(&t, "foo", &["snes".to_string()]);
        assert!(errors
            .iter()
            .any(|e| e.contains("systems entry 'not_a_real_slug'")));
    }

    #[test]
    fn wildcard_systems_cannot_mix_with_slugs() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.VIEW"
            data_mode = "data"
            systems = ["*", "snes"]
            "#,
        )
        .unwrap();
        let errors = validate(&t, "foo", &["snes".to_string()]);
        assert!(errors.iter().any(|e| e.contains("must be the only entry")));
    }

    #[test]
    fn applies_to_wildcard_matches_any_system() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.VIEW"
            data_mode = "data"
            systems = ["*"]
            "#,
        )
        .unwrap();
        assert!(applies_to(&t, "snes"));
        assert!(applies_to(&t, "n64"));
    }

    #[test]
    fn applies_to_concrete_list_matches_only_listed_systems() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.VIEW"
            data_mode = "data"
            systems = ["psx"]
            "#,
        )
        .unwrap();
        assert!(applies_to(&t, "psx"));
        assert!(!applies_to(&t, "snes"));
    }

    #[test]
    fn resolve_substitutes_placeholders() {
        let t = parse_intent(
            r#"
            id = "retroarch"
            display_name = "RetroArch"
            package = "com.retroarch"
            activity = "com.retroarch.browser.retroactivity.RetroActivityFuture"
            action = "android.intent.action.MAIN"
            data_mode = "extra"
            data_extra_name = "ROM"
            systems = ["*"]
            flags = ["FLAG_ACTIVITY_CLEAR_TOP"]

            [[extras]]
            name = "ROM"
            type = "string"
            value = "{rom_uri}"

            [[extras]]
            name = "LIBRETRO"
            type = "string"
            value = "{core}"

            [[extras]]
            name = "PATH_ECHO"
            type = "string"
            value = "literal {{brace}} {rom_path}"
            "#,
        )
        .unwrap();

        let ctx = LaunchContext {
            rom_uri: "content://com.relic/rom.zip",
            rom_path: "snes/game.zip",
            core: Some("/data/data/com.retroarch/cores/snes9x_libretro_android.so"),
        };
        let resolved = resolve(&t, "snes", &ctx);

        assert_eq!(resolved.package, "com.retroarch");
        assert_eq!(resolved.extras[0].value, "content://com.relic/rom.zip");
        assert_eq!(
            resolved.extras[1].value,
            "/data/data/com.retroarch/cores/snes9x_libretro_android.so"
        );
        assert_eq!(resolved.extras[2].value, "literal {brace} snes/game.zip");
        // Implicit flags added, existing ones preserved.
        assert!(resolved
            .flags
            .iter()
            .any(|f| f == "FLAG_ACTIVITY_CLEAR_TOP"));
        assert!(resolved
            .flags
            .iter()
            .any(|f| f == "FLAG_GRANT_READ_URI_PERMISSION"));
        assert!(resolved.flags.iter().any(|f| f == "FLAG_ACTIVITY_NEW_TASK"));
    }

    #[test]
    fn resolve_applies_per_system_override() {
        let t = parse_intent(
            r#"
            id = "foo"
            display_name = "Foo"
            package = "com.example.foo"
            activity = "com.example.foo.MainActivity"
            action = "android.intent.action.VIEW"
            data_mode = "data"
            systems = ["gamecube", "wii"]

            [[extras]]
            name = "DEFAULT"
            type = "string"
            value = "base"

            [per_system.wii]
            activity = "com.example.foo.WiiActivity"
            extras = [
              { name = "WII_ONLY", type = "string", value = "{rom_uri}" },
            ]
            "#,
        )
        .unwrap();
        let ctx = LaunchContext {
            rom_uri: "content://x",
            rom_path: "wii/game.iso",
            core: None,
        };

        let base = resolve(&t, "gamecube", &ctx);
        assert_eq!(base.activity, "com.example.foo.MainActivity");
        assert_eq!(base.extras.len(), 1);
        assert_eq!(base.extras[0].name, "DEFAULT");

        let overridden = resolve(&t, "wii", &ctx);
        assert_eq!(overridden.activity, "com.example.foo.WiiActivity");
        assert_eq!(overridden.extras.len(), 1);
        assert_eq!(overridden.extras[0].name, "WII_ONLY");
        assert_eq!(overridden.extras[0].value, "content://x");
    }
}
