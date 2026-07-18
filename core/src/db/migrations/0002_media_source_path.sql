-- Media rows must record where the source asset lives so the cache is fully
-- rebuildable from the DB alone (docs/media-conventions.md §6). cache_hash
-- may be "" for kinds served straight from the source (video, for now).
ALTER TABLE media ADD COLUMN source_path TEXT;
