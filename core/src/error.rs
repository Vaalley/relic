use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid system definition {name}: {reason}")]
    SystemDef { name: String, reason: String },

    #[error("invalid intent template {id}: {reason}")]
    IntentDef { id: String, reason: String },

    #[error("unknown system slug: {0}")]
    UnknownSystem(String),

    #[error("library not found: id {0}")]
    LibraryNotFound(i64),

    #[error("schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: i64, supported: i64 },

    #[error("game not found: id {0}")]
    GameNotFound(i64),

    #[error("no launch profile configured for system '{0}' on this platform")]
    NoLaunchProfile(String),

    #[error("emulator not found: {0}")]
    EmulatorNotFound(String),

    #[error("bad launch template: {0}")]
    Template(#[from] crate::launch::template::TemplateError),

    #[error("failed to launch '{exec}': {reason}")]
    LaunchFailed { exec: String, reason: String },

    #[error("collection not found: id {0}")]
    CollectionNotFound(i64),

    #[error("collection {0} is a smart collection; games can't be added or removed directly")]
    NotManualCollection(i64),
}
