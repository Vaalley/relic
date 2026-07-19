# Vendored subset of `rcheevos`

Source: <https://github.com/RetroAchievements/rcheevos>, commit
`2ac45d357bce2906bb0f1438f3eaf8ce6e78e3c4`. MIT licensed (see `LICENSE`).

Per `docs/retroachievements-design.md` §2.2, Relic binds `rcheevos`' `rc_hash`
module via FFI rather than reimplementing RA's per-console hashing rules —
correctness matters more than a smaller vendor footprint here, since a wrong
hash is a silent match failure.

Only the `rc_hash` subsystem is vendored, and only the ROM (cartridge) path:

- `include/rc_export.h`, `include/rc_consoles.h`, `include/rc_hash.h` — public
  API + console id enum.
- `src/rc_compat.h`, `src/rc_compat.c` — cross-platform compat shims
  (`rc_mutex_*`, C89 fallbacks) `rc_hash` depends on.
- `src/rhash/rc_hash_internal.h`, `hash.c`, `hash_rom.c`, `md5.c`, `md5.h` —
  the hash engine itself and the cartridge-console hash rules.

**Deliberately excluded** (compiled out via `RC_HASH_NO_DISC`,
`RC_HASH_NO_ENCRYPTED`, `RC_HASH_NO_ZIP` in `build.rs`), so their source
files (`hash_disc.c`, `hash_encrypted.c`, `hash_zip.c`, `cdreader.c`,
`aes.c`) are not vendored at all:

- **Disc-based systems** (PS1/2/PSP, Saturn, Dreamcast, GameCube, Wii, PC-FX
  CD, Sega/Neo Geo/3DO CD, …) — design doc open question #2. These need a CD
  sector reader wired to Relic's own file/archive layer; deferred.
- **Encrypted formats** (3DS CIA/NCCH) — needs platform key material Relic
  has no source for.
- **rc_hash's own zip handling** (MS-DOS, Arduboy FX) — Relic's scanner
  already extracts archive members to a buffer and calls
  `rc_hash_generate_from_buffer` directly (design doc §2.2), so `rc_hash`
  never needs to open a zip itself.

Arcade (`RC_CONSOLE_ARCADE`) matches by ROM-set name, not content hash —
`rc_hash_arcade` is compiled in but Relic does not call it in sub-phase 6a;
set-name matching is design doc open question #3, unresolved.

To update: re-clone the commit above (or later), re-copy the same file list,
bump the commit hash here, and re-run the module's test suite — a `rc_hash`
version bump can change hash output for a console, which is exactly the
"authoritative but must track upstream" tradeoff the design doc calls out.
