//! Media discovery and thumbnail cache (Phase 1/2, PLAN.md §4.3, §5).
//!
//! Planned: discover local artwork by naming convention next to ROMs and in
//! ES-style `media/` folders; downscale into a content-addressed cache
//! (`media.cache_hash`) so grids stay smooth on weak GPUs. The DB stores
//! hashes only — cache files are disposable and rebuildable.
