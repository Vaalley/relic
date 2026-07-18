-- Schema v1 (PLAN.md §4.3).
-- Scanned tables (libraries, systems, games, files, metadata, media) are
-- rebuildable from disk. User tables (user_data, collections, play_sessions,
-- settings) are precious: rescans must never touch them destructively.

CREATE TABLE libraries (
    id         INTEGER PRIMARY KEY,
    root_uri   TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Seeded from the systems registry (core/data/systems/*.toml) on open.
CREATE TABLE systems (
    id         INTEGER PRIMARY KEY,
    slug       TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    extensions TEXT NOT NULL              -- comma-separated, lowercase, no dots
);

CREATE TABLE games (
    id             INTEGER PRIMARY KEY,
    system_id      INTEGER NOT NULL REFERENCES systems(id),
    canonical_name TEXT NOT NULL,
    sort_name      TEXT NOT NULL,
    region         TEXT
);
CREATE INDEX idx_games_system ON games(system_id, sort_name);

CREATE TABLE files (
    id         INTEGER PRIMARY KEY,
    game_id    INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    rel_path   TEXT NOT NULL,
    size       INTEGER NOT NULL,
    mtime      INTEGER NOT NULL,
    quick_key  TEXT NOT NULL,             -- size:mtime fingerprint for incremental scans
    crc32      TEXT,                      -- lazy, filled by background hasher
    md5        TEXT,                      -- lazy
    in_archive TEXT,                      -- inner path when the rom lives inside a zip/7z
    UNIQUE (library_id, rel_path, in_archive)
);
CREATE INDEX idx_files_game ON files(game_id);

CREATE TABLE metadata (
    game_id      INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    source       TEXT NOT NULL,           -- 'gamelist', 'dat', 'scraper:<provider>'
    title        TEXT,
    description  TEXT,
    genre        TEXT,
    developer    TEXT,
    publisher    TEXT,
    release_date TEXT,
    players      TEXT,
    rating       REAL,
    PRIMARY KEY (game_id, source)
);

CREATE TABLE media (
    game_id    INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,             -- boxart | screenshot | marquee | video | ...
    source     TEXT NOT NULL,
    cache_hash TEXT NOT NULL,             -- content-addressed file in the media cache
    PRIMARY KEY (game_id, kind, source)
);

-- ---------- precious tables below this line ----------

CREATE TABLE user_data (
    game_id     INTEGER PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    favorite    INTEGER NOT NULL DEFAULT 0,
    hidden      INTEGER NOT NULL DEFAULT 0,
    user_rating REAL,
    custom_name TEXT,
    notes       TEXT
);

CREATE TABLE collections (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT 'manual',  -- manual | smart
    smart_query TEXT
);

CREATE TABLE collection_games (
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    game_id       INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (collection_id, game_id)
);

CREATE TABLE emulators (
    id              INTEGER PRIMARY KEY,
    name            TEXT NOT NULL,
    platform        TEXT NOT NULL,        -- windows | macos | linux | android
    exec_or_package TEXT NOT NULL
);

CREATE TABLE launch_profiles (
    id           INTEGER PRIMARY KEY,
    emulator_id  INTEGER NOT NULL REFERENCES emulators(id) ON DELETE CASCADE,
    system_id    INTEGER NOT NULL REFERENCES systems(id),
    arg_template TEXT NOT NULL,           -- tokens: {rom} {rom_dir} {rom_extracted} {core}
    priority     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE play_sessions (
    id         INTEGER PRIMARY KEY,
    game_id    INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    started_at INTEGER NOT NULL,
    ended_at   INTEGER,
    duration_s INTEGER
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,               -- namespaced: 'core.*', 'module.<name>.*'
    value TEXT NOT NULL
);

-- Full-text search over names; kept in sync by scan/upsert code.
CREATE VIRTUAL TABLE games_fts USING fts5(
    canonical_name,
    content='games',
    content_rowid='id'
);

CREATE TRIGGER games_ai AFTER INSERT ON games BEGIN
    INSERT INTO games_fts(rowid, canonical_name) VALUES (new.id, new.canonical_name);
END;
CREATE TRIGGER games_ad AFTER DELETE ON games BEGIN
    INSERT INTO games_fts(games_fts, rowid, canonical_name)
    VALUES ('delete', old.id, old.canonical_name);
END;
CREATE TRIGGER games_au AFTER UPDATE OF canonical_name ON games BEGIN
    INSERT INTO games_fts(games_fts, rowid, canonical_name)
    VALUES ('delete', old.id, old.canonical_name);
    INSERT INTO games_fts(rowid, canonical_name) VALUES (new.id, new.canonical_name);
END;
