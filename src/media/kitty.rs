use base64::Engine;
use std::io::{self, Cursor, Write};
use std::path::Path;

const CHUNK_SIZE: usize = 4096;

pub fn is_kitty_supported() -> bool {
    if let Ok(term) = std::env::var("TERM_PROGRAM") {
        let t = term.to_lowercase();
        return t.contains("kitty") || t.contains("wezterm") || t.contains("ghostty");
    }
    if let Ok(term) = std::env::var("TERM") {
        return term.contains("kitty") || term.contains("xterm-kitty");
    }
    false
}

fn load_as_png(path: &Path) -> io::Result<Vec<u8>> {
    let img =
        image::open(path).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    let mut png_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    Ok(png_bytes)
}

/// Display an image at the specified terminal row/col.
/// Omits c/r to let the terminal preserve the image's native aspect ratio.
pub fn display_image_at(path: &Path, row: u16, col: u16) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let png_data = load_as_png(path)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png_data);

    let mut stdout = io::stdout().lock();

    // Move cursor to target position
    write!(stdout, "\x1b[{};{}H", row + 1, col + 1)?;

    let chunks: Vec<&[u8]> = encoded.as_bytes().chunks(CHUNK_SIZE).collect();

    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i == chunks.len() - 1;
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");

        if i == 0 {
            // a=T = transmit+display, f=100 = PNG, t=d = direct data
            // No c/r params → terminal uses native image size, preserving aspect ratio
            write!(
                stdout,
                "\x1b_Ga=T,f=100,t=d,m={};{}\x1b\\",
                if is_last { 0 } else { 1 },
                chunk_str
            )?;
        } else {
            write!(
                stdout,
                "\x1b_Gm={};{}\x1b\\",
                if is_last { 0 } else { 1 },
                chunk_str
            )?;
        }
    }

    stdout.flush()?;
    Ok(())
}

/// Delete all images from the terminal display.
pub fn clear_images() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    // a=d, d=A = delete all visible images
    write!(stdout, "\x1b_Ga=d,d=A;\x1b\\")?;
    stdout.flush()?;
    Ok(())
}
