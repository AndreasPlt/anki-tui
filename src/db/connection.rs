use crate::error::{Error, Result};
use rusqlite::Connection;
use std::path::Path;

pub fn open_collection(path: &Path) -> Result<Connection> {
    if !path.exists() {
        return Err(Error::CollectionNotFound(path.to_path_buf()));
    }

    let conn = Connection::open(path).map_err(|e| {
        if is_locked(&e) {
            Error::DbLocked(e)
        } else {
            Error::Db(e)
        }
    })?;

    // Anki uses WAL mode — permits concurrent readers
    conn.pragma_update(None, "journal_mode", "wal")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;

    // Register unicase collation (Anki uses this for deck name ordering)
    conn.create_collation("unicase", |a: &str, b: &str| {
        a.to_lowercase().cmp(&b.to_lowercase())
    })?;

    Ok(conn)
}

fn is_locked(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ffi::ErrorCode::DatabaseBusy,
                ..
            },
            _
        )
    )
}
