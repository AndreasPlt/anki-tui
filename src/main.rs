mod app;
mod db;
mod error;
mod media;
mod proto;
mod scheduler;
mod template;
mod tui;

use error::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let (collection_path, media_dir) = resolve_paths(&args)?;

    let mut app = app::App::new(&collection_path, media_dir)?;
    let mut terminal = tui::terminal::init().map_err(error::Error::Io)?;

    let result = app.run(&mut terminal);

    // Always restore terminal, even on error
    let _ = tui::terminal::restore();

    result
}

fn resolve_paths(args: &[String]) -> Result<(PathBuf, PathBuf)> {
    // Check for --collection <path> argument
    let collection_path = if let Some(pos) = args.iter().position(|a| a == "--collection") {
        args.get(pos + 1)
            .map(PathBuf::from)
            .ok_or_else(|| {
                error::Error::CollectionNotFound(PathBuf::from(
                    "--collection requires a path",
                ))
            })?
    } else {
        // Auto-detect Anki collection
        find_default_collection()?
    };

    // Check for --media-dir <path> override
    let media_dir = if let Some(pos) = args.iter().position(|a| a == "--media-dir") {
        args.get(pos + 1)
            .map(PathBuf::from)
            .unwrap_or_default()
    } else {
        // Default: sibling to collection file, or fall back to real Anki media dir
        let sibling = collection_path
            .parent()
            .map(|p| p.join("collection.media"))
            .unwrap_or_default();
        if sibling.exists() {
            sibling
        } else {
            // Fall back to default Anki profile media dir
            find_default_media_dir().unwrap_or(sibling)
        }
    };

    Ok((collection_path, media_dir))
}

fn find_default_collection() -> Result<PathBuf> {
    let base = dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Library/Application Support")))
        .ok_or_else(|| {
            error::Error::CollectionNotFound(PathBuf::from("cannot determine data directory"))
        })?;

    let anki_dir = base.join("Anki2");
    if !anki_dir.exists() {
        return Err(error::Error::CollectionNotFound(anki_dir));
    }

    // Find first profile with a collection
    let entries = std::fs::read_dir(&anki_dir)
        .map_err(|_| error::Error::CollectionNotFound(anki_dir.clone()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let collection = path.join("collection.anki2");
            if collection.exists() {
                return Ok(collection);
            }
        }
    }

    Err(error::Error::CollectionNotFound(anki_dir))
}

fn find_default_media_dir() -> Option<PathBuf> {
    let base = dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Library/Application Support")))?;
    let anki_dir = base.join("Anki2");
    for entry in std::fs::read_dir(&anki_dir).ok()?.flatten() {
        let media = entry.path().join("collection.media");
        if media.exists() {
            return Some(media);
        }
    }
    None
}
