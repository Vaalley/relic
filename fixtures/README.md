# Fixtures

Synthetic test libraries used by `relic-core`, `relic-cli`, and CI. Nothing
here is a real ROM or copyrighted asset:

- ROM files are tiny placeholder text files (16 bytes: `relic test fixture`)
  named like real releases so filename-matching and extension-filtering
  logic can be exercised realistically.
- `gamelist.xml` files are hand-written, minimal, valid ES/ES-DE-style XML —
  not extracted from any real frontend database.

## `fixtures/mini/`

A tiny multi-system library following the `<root>/<slug>/...` convention
(PLAN.md §4.4):

```
mini/
├── snes/
│   ├── Super Mario World (USA).sfc
│   └── gamelist.xml
├── nes/
│   └── Contra (USA).nes
└── gb/
    └── Tetris (World).gb
```

Used by scan/import tests and as a quick manual smoke-test target, e.g.:

```
cargo run -p relic-cli -- scan --db relic.db fixtures/mini
```
