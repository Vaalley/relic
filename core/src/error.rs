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

    #[error("unknown system slug: {0}")]
    UnknownSystem(String),

    #[error("library not found: id {0}")]
    LibraryNotFound(i64),

    #[error("schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: i64, supported: i64 },
}
