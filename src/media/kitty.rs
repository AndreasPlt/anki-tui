use base64::Engine;
use std::io::{self, Cursor, Write};
use std::path::Path;

const CHUNK_SIZE: usize = 4096;

fn is_tmux() -> bool {
    std::env::var("TMUX")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Check if the outer terminal (possibly behind tmux) supports Kitty graphics.
fn check_term_program(val: &str) -> bool {
    let t = val.to_lowercase();
    t.contains("kitty") || t.contains("wezterm") || t.contains("ghostty")
}

/// When inside tmux, try to discover the outer terminal's TERM_PROGRAM.
fn outer_terminal_supports_kitty() -> bool {
    // $TERM_PROGRAM may still be inherited from the outer shell
    if let Ok(term) = std::env::var("TERM_PROGRAM") {
        if check_term_program(&term) {
            return true;
        }
    }
    // Ask tmux for the outer terminal's TERM_PROGRAM
    if let Ok(output) = std::process::Command::new("tmux")
        .args(["show-environment", "TERM_PROGRAM"])
        .output()
    {
        // Output format: "TERM_PROGRAM=ghostty\n" or "-TERM_PROGRAM\n" (unset)
        if let Ok(s) = std::str::from_utf8(&output.stdout) {
            if let Some(val) = s.trim().strip_prefix("TERM_PROGRAM=") {
                return check_term_program(val);
            }
        }
    }
    // Can't determine — assume support (user opted into allow-passthrough)
    true
}

pub fn is_kitty_supported() -> bool {
    if let Ok(term) = std::env::var("TERM_PROGRAM") {
        if check_term_program(&term) {
            return true;
        }
    }
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("kitty") || term.contains("xterm-kitty") {
            return true;
        }
    }
    if is_tmux() {
        return outer_terminal_supports_kitty();
    }
    false
}

/// Write a Kitty graphics escape sequence, wrapping in tmux DCS passthrough if needed.
///
/// `payload` is the content between `\x1b_G` and `\x1b\\`, e.g. `"a=T,f=100,t=d,m=0;base64data"`.
fn write_kitty_escape(stdout: &mut impl Write, payload: &str) -> io::Result<()> {
    if is_tmux() {
        // Wrap in DCS passthrough: \x1bPtmux;\x1b\x1b_G{payload}\x1b\x1b\\\x1b\\
        // Every \x1b in the inner sequence must be doubled.
        write!(stdout, "\x1bPtmux;")?;
        for byte in format!("\x1b_G{payload}\x1b\\").bytes() {
            if byte == 0x1b {
                stdout.write_all(b"\x1b\x1b")?;
            } else {
                stdout.write_all(&[byte])?;
            }
        }
        write!(stdout, "\x1b\\")?;
    } else {
        write!(stdout, "\x1b_G{payload}\x1b\\")?;
    }
    Ok(())
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
pub fn display_image_at(path: &Path, row: u16, col: u16) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let png_data = load_as_png(path)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&png_data);

    let mut stdout = io::stdout().lock();

    // Cursor positioning — standard CSI, no tmux wrapping needed
    write!(stdout, "\x1b[{};{}H", row + 1, col + 1)?;

    let chunks: Vec<&[u8]> = encoded.as_bytes().chunks(CHUNK_SIZE).collect();

    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i == chunks.len() - 1;
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        let m = if is_last { 0 } else { 1 };

        let payload = if i == 0 {
            format!("a=T,f=100,t=d,m={m};{chunk_str}")
        } else {
            format!("m={m};{chunk_str}")
        };
        write_kitty_escape(&mut stdout, &payload)?;
    }

    stdout.flush()?;
    Ok(())
}

/// Delete all images from the terminal display.
pub fn clear_images() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    write_kitty_escape(&mut stdout, "a=d,d=A;")?;
    stdout.flush()?;
    Ok(())
}
