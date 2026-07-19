-- Module-owned tables (docs/retroachievements-design.md §4.2). Not part of
-- the core schema; versioned via the settings key
-- 'module.retroachievements.schema_version', never PRAGMA user_version.
-- No foreign keys to core tables: `relic_file_id` is a module-owned link to
-- core's `files.id`, deliberately without an FK constraint so this module
-- can be fully removed by dropping every `ra_` table with no core-schema
-- side effects (design doc §4.3, "the de-integration rule").

-- Sub-phase 6a (hashing + matching, T1 anonymous): the table this phase
-- actually writes to.
CREATE TABLE ra_games (
    ra_game_id     INTEGER PRIMARY KEY,
    console_id     INTEGER,
    title          TEXT,
    hash           TEXT NOT NULL,
    relic_file_id  INTEGER NOT NULL,
    matched_at     INTEGER NOT NULL,
    last_synced_at INTEGER,
    UNIQUE(hash, relic_file_id)
);

-- Sub-phase 6b (read-only API display, T1): achievement metadata cache.
CREATE TABLE ra_achievements (
    ra_achievement_id INTEGER PRIMARY KEY,
    ra_game_id        INTEGER NOT NULL,
    title             TEXT,
    description       TEXT,
    points            INTEGER,
    badge_url         TEXT,
    display_order     INTEGER,
    UNIQUE(ra_achievement_id, ra_game_id)
);

-- Sub-phase 6c (login-backed progress, T2).
CREATE TABLE ra_auth (
    username      TEXT PRIMARY KEY,
    api_key_enc   BLOB,
    api_key_nonce BLOB,
    points_total  INTEGER,
    last_login_at INTEGER
);

-- Sub-phase 6c.
CREATE TABLE ra_user_unlocks (
    ra_achievement_id INTEGER NOT NULL,
    username          TEXT NOT NULL,
    unlocked_at       INTEGER NOT NULL,
    hardcore          INTEGER NOT NULL,
    PRIMARY KEY(ra_achievement_id, username, hardcore)
);

-- Rate-limit/backoff bookkeeping (design doc §3.2), used from 6b onward.
CREATE TABLE ra_sync_log (
    endpoint    TEXT NOT NULL,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,
    status      TEXT NOT NULL,
    http_status INTEGER,
    error       TEXT,
    PRIMARY KEY(endpoint, started_at)
);
