-- Cache for hashes that did not return a match from RetroAchievements (design doc §2.3),
-- preventing repeated API calls for unknown games.
CREATE TABLE ra_no_match (
    hash       TEXT PRIMARY KEY,
    checked_at INTEGER NOT NULL
);
