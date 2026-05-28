use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("database locked — is Anki desktop open?\n{0}")]
    DbLocked(rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("protobuf decode error: {0}")]
    ProtoDecode(String),

    #[error("template error: {0}")]
    Template(String),

    #[error("no cards due in this deck")]
    NoDueCards,

    #[error("collection not found at {0}")]
    CollectionNotFound(PathBuf),
}

pub type Result<T> = std::result::Result<T, Error>;
