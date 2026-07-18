# RetroAchievements Module — Design

> Status: **Draft, Phase 6**
> Companion to `PLAN.md` §7.2 (RetroAchievements module), §4.2 (lazy hashing),
> §1 (privacy tiers). Implements `modules/retroachievements/src/lib.rs` stub.
> Voice and conventions match `PLAN.md`; this is a design document, not code.

Relic is a local-first, zero-telemetry launcher. The RetroAchievements (RA)
module is an opt-in companion: T1 (anonymous fetch) for display, T2 (login) for
per-user progress. It is compiled out by default (`--no-default-features` builds
a pure-T0 binary) and lives entirely in `modules/retroachievements/`. The core
engine never imports it.

---

## 1. Scope and honest limits

A **frontend cannot unlock achievements.** Unlocking happens inside RA-aware
emulator cores (RetroArch with `rcheevos`, and RA-enabled standalones) that
hook the emulated game's memory and post awards to the RA server. Relic does
not emulate and does not bundle cores, so it has no path to awarding anything.

Relic's role is **companion and motivator**:

- Identify local games against RA's game catalog (per-console hashing → RA game
  id), so the launcher can show "this game has achievements" without the user
  doing anything beyond opting in.
- With a T2 login, display per-game achievement lists, the user's unlock state,
  points, and mastery badges — all from cached API responses.
- Surface that data where a launcher already has UI: badges on grid tiles,
  filters, and smart collections ("close to mastery", "unplayed with cheevos").
- On launch via RetroArch, *optionally* check that the user's RA hardcore
  preference matches their RetroArch config — **informational only**, never
  enforced. Relic does not control emulator settings.

**Non-goals (explicit):**

- No achievement unlocking, no hardcore-mode enforcement, no save-state
  policing. Relic does not inspect emulator state.
- No telemetry beyond user-initiated RA API calls. No analytics on what the
  user plays, no crash reporting home, no "anonymous usage" channel. The only
  network egress is the RA API, and only when the user triggers it (or has
  opted into a scheduled sync — see §3).
- No bundling of RA credentials for shared accounts; one user, one key.
- No reimplementation of the RA rich presence or leaderboard submit protocols —
  those are the emulator's job.

---

## 2. Game identification

### 2.1 RA hashes are console-specific, not "MD5 of the file"

RA does **not** use a single hashing algorithm. The `rcheevos` C library's
`rc_hash` module selects an algorithm per console id:

- **Plain MD5 of the whole file** for many cartridge systems (e.g. Genesis,
  SNES, most things without a header that RA strips) [VERIFY exact console
  list against current `rc_hash` source].
- **Header-skipping MD5** for systems where a fixed-size header must be removed
  before hashing (NES iNES headers, Atari Lynx headers, etc.) [VERIFY header
  sizes and which consoles are header-skipped].
- **Disc-specific algorithms** for CD systems: hash of a specific sector range
  / TOC / a hash of the disc image minus padding, varying by PlayStation,
  Saturn, PC-Engine CD, Mega-CD, etc. [VERIFY per-system CD rules].
- **Arcade** is matched by ROM set / parent set name rather than a content
  hash; RA's arcade handling differs from the cartridge path [VERIFY].

This is why `core/src/scan/hash.rs` deliberately computes only a generic
whole-file `(crc32, md5)` and defers console-aware hashing to this module —
its module doc says so explicitly. The generic MD5 is *not* the RA hash for
most consoles; it is a fallback identifier and a building block.

### 2.2 Recommendation: bind `rcheevos`' `rc_hash` via FFI

Two options were considered:

1. **Bind `rcheevos` (C) via FFI** and call `rc_hash_generate_from_file` /
   `rc_hash_generate_from_buffer` per console. The library already encodes
   every console's rules and is maintained by the RA project upstream.
2. **Reimplement the subset in Rust.** ~10 console algorithms, header tables,
   CD-sector readers. Smaller binary, no C dependency, full auditability.

**Recommendation: option 1, bind `rcheevos`.** Trade-offs:

| | Bind `rcheevos` (recommended) | Reimplement in Rust |
|---|---|---|
| Correctness | Authoritative; matches RA server exactly | Must track upstream rule changes; risk of mismatched hashes → silent match failures |
| Maintenance | Re-vendor `rcheevos` periodically | ~10 algorithms to keep in sync; CD systems are fiddly |
| Binary size | +~100–300 KB of C [VERIFY] | Negligible |
| Audit / supply chain | One extra C dependency to vendor and review | Pure Rust, easier to audit |
| Pillars | "Modular" ✓ (it's an optional module), "Reliable" ✓ (correct hashes), "Small" ⚠ (small C bump, gated behind the feature) | "Small" ✓, "Reliable" ⚠ (drift risk) |

The "small" pillar cost is acceptable because the entire module is compiled out
by default — the C dependency only lands in binaries that opted into RA. The
"reliable" pillar wins the argument: a wrong hash produces a silent
no-match, which is worse than a slightly larger module binary.

Binding mechanics: vendor `rcheevos` as a `cc`/`cmake`-built static lib under
`modules/retroachievements/native/`, expose a thin `rc_hash` wrapper through
`unsafe` Rust in the module crate. **In-archive ROM bytes are already
reachable** — the core scanner has zip/7z support and can stream extracted
bytes to the hasher without writing to disk, so `rc_hash_generate_from_buffer`
covers archived ROMs the scanner already enumerated (PLAN.md §4.2
"archive-aware"). For files on disk, `rc_hash_generate_from_file` reads
directly.

The generic whole-file MD5 from `core::scan::hash` is reused as a fast
pre-filter and as the fallback for consoles RA treats as "plain MD5 of the
whole file" — avoiding a second full read when the two coincide [VERIFY which
consoles this optimization is valid for].

### 2.3 Hash → RA game id

RA exposes a **hash library** endpoint: submit a hash, get back the RA game id
(and console id, title) if it is known. The mapping is many-hashes → one
game id (a game can have multiple valid dumps). Relic queries this on demand
the first time a game is seen with the RA module enabled, then caches the
result (see §4). Unknown hashes are cached as "no match" with a TTL so we
don't re-query the server every launch.

### 2.4 Where hashes live

RA hashes are **module-owned data**, not core schema. They live in the
`ra_game_hashes` table (see §4) inside the same SQLite DB, but under the
module's `ra_` prefix and migration namespace. The core `files.md5` column
holds the generic whole-file hash; the RA-specific hash is a separate column
in a module table, linked back to `files.id`. This keeps the core schema
untouched if the module is removed.

---

## 3. API client design

### 3.1 Endpoints for v1

RA has two API surfaces; v1 uses a mix [VERIFY all endpoint names, params, and
response shapes against current RA docs]:

| Purpose | Endpoint | Auth | Tier |
|---|---|---|---|
| Login (exchange username + password for a web API key, *or* validate a user-supplied key) | `dorequest.php?r=login` (legacy) **or** `retroachievements.org/API/...` with the Web API key | username + password / API key | T2 |
| Hash → game lookup (the "hash library") | `dorequest.php?r=hashlibrary&...` or the Web API hash lookup [VERIFY exact name] | API key | T1 (anonymous) / T2 |
| Game achievement list + user progress | `API_GetGameInfoAndUserProgress` | API key (T2 for progress; anonymous for list only [VERIFY]) | T1/T2 |
| User summary (points, recent unlocks, mastery count) | `API_GetUserSummary` | API key | T2 |
| Per-console game list (for browsing "all RA games on SNES") | `API_GetGameList` | API key | T1 |

v1 ships the first four; `API_GetGameList` is optional and only if the
"browse RA catalog" UX proves wanted.

### 3.2 Rate-limit etiquette and backoff

- RA documents rate limits; the client honors them [VERIFY current limits].
  Conservative default: ≤ 5 requests/sec, ≤ 200/min, with exponential backoff
  + jitter on `429`/`5xx`, capped at 60 s. A 10-min circuit breaker opens
  after repeated failures so a flaky network doesn't spam.
- All requests are serial per module instance (one in-flight at a time) unless
  the user explicitly batch-syncs a whole library, in which case a small
  bounded concurrency (e.g. 2) is used. Bulk library sync shows a progress UI
  and is cancellable.
- Every response carries a `last_synced_at`; the cache is the source of truth
  for display. The network is never on the critical path of browsing.

### 3.3 When requests happen

- **Never at startup** without explicit opt-in. Enabling the module in
  settings is the opt-in; even then, no requests fire until the user opens an
  RA-aware screen or triggers a sync.
- **On demand**: opening a game detail page for a game with a cached RA link
  refreshes that game's progress (T2) if the cached copy is older than the
  staleness threshold (default 1 hour; configurable; "refresh now" button
  always available).
- **On launch**: when launching a game via RetroArch, optionally pre-fetch the
  game's achievement list so the user sees current unlock state before they
  start playing. Off by default; a setting.
- **Scheduled sync**: optional, off by default. "Sync my progress every N
  hours while Relic is open." Never a background daemon; only while the app is
  running.

---

## 4. Local cache schema

All RA tables are **module-owned**, prefixed `ra_`, and live in the same
SQLite file as the core schema. The core migration runner
(`core/src/db/mod.rs`) owns `user_version` and the `0001_…`, `0002_…` files;
modules must not extend that list.

### 4.1 Proposed module migration namespace

Mechanism: a **per-module `user_version`-equivalent** stored in the
`settings` table under a module-namespaced key, e.g.
`settings('ra.schema_version') = N`. Each module ships its own append-only
migration list (e.g. `modules/retroachievements/migrations/ra_0001.sql` …)
and its own tiny migration runner that:

1. Reads `settings.value` for its schema-version key (default 0).
2. Applies its own SQL files in order inside a transaction, bumping the
   settings key per step.
3. Never touches `PRAGMA user_version` and never creates tables outside its
   prefix.

This keeps the core's append-only migration rule (PLAN.md hard rule 5)
intact, gives modules the same append-only discipline, and lets a module be
removed cleanly by dropping every table it owns (see §4.3). The mechanism is
generic and should be promoted to a small `ModuleMigrations` helper in
`core::db` so future modules (scraper, sync) reuse it rather than
reimplementing it — but that helper lives in core and is module-agnostic; it
does not know RA's table names.

### 4.2 Tables (sketch, v1)

```
ra_auth(username TEXT PRIMARY KEY,
        api_key_enc BLOB,            -- encrypted-at-rest, see §4.4
        api_key_nonce BLOB,
        points_total INTEGER,
        last_login_at INTEGER)

ra_games(ra_game_id INTEGER PRIMARY KEY,
         console_id INTEGER,
         title TEXT,
         hash TEXT,                  -- the RA hash that matched
         relic_file_id INTEGER,      -- FK -> core files.id (no ON DELETE; module-managed)
         matched_at INTEGER,
         last_synced_at INTEGER,
         UNIQUE(hash, relic_file_id))

ra_achievements(ra_achievement_id INTEGER PRIMARY KEY,
                ra_game_id INTEGER,
                title TEXT,
                description TEXT,
                points INTEGER,
                badge_url TEXT,       -- cached, downloaded to media cache
                display_order INTEGER,
                UNIQUE(ra_achievement_id, ra_game_id))

ra_user_unlocks(ra_achievement_id INTEGER,
                username TEXT,
                unlocked_at INTEGER,  -- epoch seconds, from API
                hardcore INTEGER,     -- 0/1
                PRIMARY KEY(ra_achievement_id, username, hardcore))

ra_sync_log(endpoint TEXT,
            started_at INTEGER,
            finished_at INTEGER,
            status TEXT,              -- 'ok' | 'error' | 'throttled'
            http_status INTEGER,
            error TEXT,
            PRIMARY KEY(endpoint, started_at))
```

`ra_game_hashes` (§2.4) is folded into `ra_games` via the `hash` + `relic_file_id`
link; if a single file can produce multiple candidate hashes (e.g. multi-disc),
a separate `ra_game_hashes` table is preferred — defer to the implementation
spike.

### 4.3 The de-integration rule

**Dropping every `ra_` table, deleting the `ra.schema_version` settings row,
and removing the module's feature flag fully de-integrates RA.** No core
table references `ra_*`; the `relic_file_id` link is module-owned (no core
foreign key). A `relic-cli ra disable` command (or "Remove RA integration"
button) runs exactly this drop and clears any in-memory state. After it, the
binary behaves identically to one compiled with `--no-default-features`
regarding RA — no menus, no badges, no network.

### 4.4 API key storage

The RA web API key is a bearer token that can unlock achievements *if used by
an emulator*; treat it as a secret. It is **encrypted at rest** with a
key derived from a per-library passphrase *or* the OS keychain (via
`keyring`-crate equivalent on each platform) when available [VERIFY which
platforms get keychain vs passphrase fallback]. The `ra_auth` row stores
`api_key_enc` + `api_key_nonce` (authenticated symmetric encryption, e.g.
`chacha20poly1305`); the plaintext key is held in memory only while the
module is active and the user is logged in. On logout, the row is deleted.

The username is stored in plaintext (it is not secret, and is needed to
construct per-user API calls and to scope `ra_user_unlocks`).

---

## 5. UX surface (consumed by shells later)

The module exposes data through the engine's query/event surface; shells
render it. No shell ships RA-specific code that breaks when the module is
absent — capabilities gate the UI (PLAN.md §3 "Shells discover module
capabilities at runtime").

- **Per-game achievement list**: on the game detail page, a section listing
  achievements with title, description, points, badge, and unlock state
  (locked / unlocked / hardcore-unlocked). Sorted by `display_order`.
- **Mastery badge on grid tiles**: a small overlay icon for games the logged-in
  user has mastered (all achievements unlocked, regular or hardcore —
  distinguishable). Computed from `ra_user_unlocks` joined to
  `ra_achievements` for that `ra_game_id`.
- **"Has cheevos" filter**: a query filter `ra.has_cheevos = true` available
  only when the module is enabled. Maps to "exists in `ra_games`".
- **Smart collections** (PLAN.md §5 "manual + smart collections"):
  - "Close to mastery" — games where the user has unlocked ≥ N% (default 80%)
    but not 100%.
  - "Unplayed with cheevos" — `play_sessions` empty AND `ra_games` row exists.
  - "Has achievements I haven't unlocked" — `ra_games` row exists AND not all
    `ra_achievements` are in `ra_user_unlocks` for this user.
  These are saved queries; the module contributes filter predicates to the
  smart-collection query language.
- **Offline behavior**: all of the above work offline from cache. Each RA
  data view shows a staleness indicator ("synced 3h ago", "synced just now",
  "offline — last synced 2d ago"). The "refresh now" action is always
  available; if it fails, the cached view remains usable.

---

## 6. Privacy

### 6.1 What leaves the device, and when

| Data | Leaves when | Destination | Required? |
|---|---|---|---|
| RA username | Any T2 API call | `retroachievements.org` | Only if user logs in (T2) |
| RA web API key | Any T2 API call (as auth header/param) | `retroachievements.org` | Only if user logs in (T2) |
| ROM content hash (RA-format) | Hash-library lookup, first time a game is seen with RA enabled | `retroachievements.org` | Only when module enabled and game is opened/synced |
| RA game id, achievement metadata | (Inbound) response to the above | from `retroachievements.org` | n/a |
| User's unlock progress query (username + game ids) | T2 progress sync | `retroachievements.org` | Only on T2 sync |
| Library contents, file paths, playtime, favorites, collections, anything else | **Never** | — | — |

Nothing else leaves the device. No analytics, no "anonymous" metrics, no
crash reports. The module does not log the user's play activity to RA (only
the emulator does, when it unlocks). Relic's play-session tracking stays
local.

### 6.2 Why hashes, and what that implies

The RA hash is the identifier RA itself uses; sending it is the only way to
ask "does RA know this game?". A hash is **not** reversible into the ROM
contents (MD5 of a header-stripped file is still a one-way function), but it
is **linkable**: RA can observe that this device queried this hash, and
correlate repeated queries. That is inherent to using RA at all — the
emulator sends the same hash when unlocking. Opting into RA is opting into
RA's visibility of which games you play; Relic does not add to that surface.

### 6.3 Opt-in copy suggestions (for the first-run RA screen)

- "Relic talks to RetroAchievements only when you ask it to. Your ROM
  hashes are sent so RA can identify your games — the same thing your
  emulator already does when you unlock achievements. Nothing else about
  your library leaves your device."
- "Login is optional. Without it, Relic can still show which of your games
  have achievements. With it, Relic shows your progress and mastery badges."
- "You can remove RetroAchievements integration at any time. Relic will drop
  its cached data and stop all network calls to RA."

---

## 7. Phased implementation plan

Three sub-phases, sequential. Each has hard acceptance criteria; later
sub-phases don't start until the prior one passes.

### 7.1 Sub-phase 6a — Hashing + matching (offline-capable)

- Vendor `rcheevos` under `modules/retroachievements/native/`; wire
  `rc_hash` through `unsafe` Rust.
- Implement the module migration runner (§4.1) and the `ra_games` /
  `ra_game_hashes` tables.
- Compute RA hashes for already-scanned files on demand, reusing the core's
  archive-streaming so in-zip ROMs hash without extraction.
- Hash → RA game id matching via the hash-library endpoint, with cached
  "no match" TTL.
- **No login yet.** This sub-phase is T1 (anonymous fetch) only.

**Acceptance:**

- `cargo build -p relic-retroachievements` green on Win/mac/Linux.
- `cargo build --workspace --no-default-features` still green (module is
  cleanly compiled out).
- A test fixture with known RA hashes [VERIFY we can construct synthetic
  fixtures that produce a deterministic RA hash without shipping real ROM
  content — likely needs crafted header-stripped buffers] matches the
  expected RA game id against a recorded API response (VCR-style test, no
  network in CI).
- Dropping all `ra_` tables and re-enabling the module re-populates from
  scratch with no core-schema side effects.
- Offline: with the cache populated and network disabled, "has cheevos"
  filtering works.

### 7.2 Sub-phase 6b — Read-only API display (T1)

- Achievement metadata fetch + cache (`ra_achievements`).
- Badge image download into the core media cache (content-addressed; same
  pipeline as scraper media per PLAN.md §7.1).
- Query predicates and smart collections from §5.
- Staleness indicators and "refresh now".
- Rate-limit/backoff client from §3.2.

**Acceptance:**

- Per-game achievement list renders offline from cache after one successful
  sync.
- "Has cheevos" filter and the three smart collections from §5 return correct
  results on a 1k-game fixture with a recorded API corpus.
- Rate-limit backoff tested with a mock server returning `429` then `200`.
- No network call fires on app startup with the module enabled but no user
  action (verified by a test that asserts zero HTTP requests during
  `Engine::open` + `list_systems`).

### 7.3 Sub-phase 6c — Login-backed progress (T2)

- Login flow (validate user-supplied API key, or username+password exchange
  [VERIFY whether v1 supports password exchange or key-only]).
- API key encrypted-at-rest per §4.4; keychain integration where available.
- `ra_user_unlocks` population from `API_GetUserInfoAndUserProgress` /
  `API_GetUserSummary` [VERIFY exact endpoint for per-game user progress].
- Mastery badges on grid tiles (regular vs hardcore distinction).
- Optional on-launch pre-fetch and scheduled sync (both off by default).
- "Remove RA integration" command that drops all `ra_` tables + settings row.

**Acceptance:**

- Login persists across restarts; logout clears `ra_auth`.
- Mastery badge appears for a fixture user who has unlocked all achievements
  for a fixture game (recorded API response).
- Hardcore vs regular mastery is visually distinct.
- "Remove RA integration" leaves the DB with zero `ra_` tables, zero `ra.*`
  settings keys, and a core schema version unchanged from before the module
  was ever enabled (verified by `PRAGMA user_version` assertion).
- `cargo build --workspace --no-default-features` still green.

---

## 8. Open questions

1. **`rc_hash` console coverage**: which consoles does the generic whole-file
   MD5 from `core::scan::hash` coincide with the RA hash, so we can skip a
   second read? [VERIFY against `rc_hash` source.]
2. **CD-system hashing**: do RA's CD algorithms require the disc image, the
   cue/bin, or both? How does this interact with the scanner's m3u/multi-disc
   grouping (PLAN.md §4.2)? [VERIFY.]
3. **Arcade matching**: RA matches arcade by set name, not content hash. Is
   that in scope for v1, or do we mark arcade as "RA not supported in Relic
   v1" and revisit? [VERIFY RA's current arcade matching.]
4. **API surface drift**: RA has a legacy `dorequest.php` and a newer
   `retroachievements.org/API/` Web API. Which endpoints exist on which, and
   which are deprecated? v1 should pick one surface and stick to it.
   [VERIFY against current RA docs.]
5. **Rate limits**: what are RA's current documented rate limits? The §3.2
   defaults are conservative guesses. [VERIFY.]
6. **API key vs password**: does the Web API still support username+password
   login, or is it key-only now? Affects the first-run login UX.
   [VERIFY.]
7. **Per-user progress endpoint**: is `API_GetGameInfoAndUserProgress` the
   right call for "this game's achievements + my unlocks", or is it
   `API_GetUserProgress` + a separate game call? [VERIFY.]
8. **Synthetic hash fixtures**: can we craft a buffer that produces a known
   RA hash for a known RA game id without shipping real ROM content (which
   would violate the fixtures rule, PLAN.md hard rule 4)? If not, the test
   strategy for 6a needs a different approach (e.g. mock the hash-library
   endpoint and assert on the *computed* hash only, not the matched id).
9. **Module migration helper**: should the per-module migration runner
   described in §4.1 be promoted to `core::db` now, or wait until a second
   module needs it? Promoting now is cleaner; waiting avoids speculative
   abstraction.
10. **Keychain fallback**: on platforms without an OS keychain, is a
    per-library passphrase acceptable UX, or should we fall back to
    plaintext-with-warning? The §4.4 proposal is passphrase; confirm during
    6c UX work.
11. **Multi-user**: RA supports one account per Relic install in v1. Is
    multi-account ever wanted (e.g. family handheld)? Out of scope for 6c;
    revisit if requested.
12. **RA hardcore check on launch**: §1 mentions an *informational* check
    that RetroArch's hardcore setting matches the user's preference. How does
    Relic read RetroArch's config safely across platforms? Defer to 6c spike.
