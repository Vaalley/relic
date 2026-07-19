-- Module-owned table (docs/retroachievements-design.md §4 documents the
-- pattern this follows). Not part of the core schema; versioned via the
-- settings key 'module.scraper.schema_version', never PRAGMA user_version.
-- No foreign key to core's games table: dropping this table must never be
-- able to affect core data, and the module must be fully removable by
-- dropping every table it owns.
CREATE TABLE scraper_matches (
    game_id     INTEGER NOT NULL,
    provider_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    confidence  TEXT NOT NULL,
    confirmed   INTEGER NOT NULL DEFAULT 0,
    matched_at  INTEGER NOT NULL,
    PRIMARY KEY (game_id, provider_id)
);
