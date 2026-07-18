# Relic themes

`themes/default/` is the bundled default theme: a layer-1 (design-token)
theme per `PLAN.md` §6 — colors (with light/dark variants), typography,
shape, and sounds, loaded from `theme.toml`.

**The `theme.toml` format is provisional.** It's shaped to match what the
core theming engine (`modules/themes`) currently expects, but the
authoritative spec — including validation rules, the `relic-cli theme
validate` command, and the eventual layer-2 declarative layout format — ships
as `docs/theme-format.md` in Phase 5. Expect fields to move or be renamed
before then.

Rules that will hold regardless of format churn (PLAN.md §6):

- Themes are pure data + assets: no network access, no filesystem access
  outside their own folder.
- A broken theme degrades to the default theme with a visible warning, never
  a crash.
