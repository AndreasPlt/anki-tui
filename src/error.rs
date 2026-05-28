use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("collection not found at {0}")]
    CollectionNotFound(PathBuf),

    #[error("sidecar error: {0}")]
    Sidecar(String),

    #[error("sidecar protocol error: {0}")]
    SidecarProtocol(String),
}

pub type Result<T> = std::result::Result<T, Error>;
