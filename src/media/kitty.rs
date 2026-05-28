use base64::Engine;
use std::io::{self, Cursor, Write};
use std::path::Path;
use std::sync::LazyLock;

const CHUNK_SIZE: usize = 4096;

/// Cached tmux nesting depth — computed once at first access.
static TMUX_DEPTH: LazyLock<usize> = LazyLock::new(detect_tmux_depth);

/// Detect how many layers of tmux we're nested inside.
fn detect_tmux_depth() -> usize {
    let tmux_val = match std::env::var("TMUX") {
        Ok(v) if !v.is_empty() => v,
        _ => return 0,
    };
    1 + parent_tmux_depth(extract_socket(&tmux_val), 5)
}

/// Recursively check parent tmux sessions for further nesting.
fn parent_tmux_depth(socket: Option<&str>, remaining: usize) -> usize {
    if remaining == 0 {
        return 0;
    }
    let Some(socket) = socket else {
        return 0;
    };
    let mut cmd = std::process::Command::new("tmux");
    cmd.arg("-S").arg(socket).args(["show-environment", "TMUX"]);
    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => return 0,
    };
    let stdout = std::str::from_utf8(&output.stdout).unwrap_or("");
    let Some(val) = stdout.trim().strip_prefix("TMUX=") else {
        return 0;
    };
    1 + parent_tmux_depth(extract_socket(val), remaining - 1)
}

/// Extract the socket path from a TMUX env value (format: "socket_path,pid,index").
fn extract_socket(tmux_val: &str) -> Option<&str> {
    let socket = tmux_val.split(',').next()?;
    if socket.is_empty() {
        None
    } else {
        Some(socket)
    }
}

fn check_term_program(val: &str) -> bool {
    let t = val.to_lowercase();
    t.contains("kitty") || t.contains("wezterm") || t.contains("ghostty")
}

/// When inside tmux, try to discover the outer terminal's TERM_PROGRAM.
fn outer_terminal_supports_kitty() -> bool {
    if std::env::var("TERM_PROGRAM")
        .map(|t| check_term_program(&t))
        .unwrap_or(false)
    {
        return true;
    }
    if let Ok(output) = std::process::Command::new("tmux")
        .args(["show-environment", "TERM_PROGRAM"])
        .output()
        && let Ok(s) = std::str::from_utf8(&output.stdout)
        && let Some(val) = s.trim().strip_prefix("TERM_PROGRAM=")
    {
        return check_term_program(val);
    }
    true
}

pub fn is_kitty_supported() -> bool {
    if std::env::var("TERM_PROGRAM")
        .map(|t| check_term_program(&t))
        .unwrap_or(false)
    {
        return true;
    }
    if let Ok(term) = std::env::var("TERM")
        && (term.contains("kitty") || term.contains("xterm-kitty"))
    {
        return true;
    }
    if *TMUX_DEPTH > 0 {
        return outer_terminal_supports_kitty();
    }
    false
}

/// Wrap raw bytes in N layers of tmux DCS passthrough.
/// Each layer doubles every `\x1b` and wraps in `\x1bPtmux;...\x1b\\`.
fn wrap_for_tmux(data: &[u8], depth: usize) -> Vec<u8> {
    let mut result = data.to_vec();
    for _ in 0..depth {
        let mut wrapped = Vec::with_capacity(result.len() * 2 + 16);
        wrapped.extend_from_slice(b"\x1bPtmux;");
        for &byte in &result {
            if byte == 0x1b {
                wrapped.extend_from_slice(b"\x1b\x1b");
            } else {
                wrapped.push(byte);
            }
        }
        wrapped.extend_from_slice(b"\x1b\\");
        result = wrapped;
    }
    result
}

/// Write a Kitty graphics escape sequence, wrapping in tmux DCS passthrough if needed.
fn write_kitty_escape(stdout: &mut impl Write, payload: &str) -> io::Result<()> {
    let depth = *TMUX_DEPTH;
    let raw = format!("\x1b_G{payload}\x1b\\").into_bytes();
    let output = wrap_for_tmux(&raw, depth);
    stdout.write_all(&output)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_for_tmux_depth_0_is_identity() {
        let input = b"\x1b_Ga=T;data\x1b\\";
        let result = wrap_for_tmux(input, 0);
        assert_eq!(result, input);
    }

    #[test]
    fn wrap_for_tmux_depth_1() {
        let input = b"\x1b_Ga=T;data\x1b\\";
        let result = wrap_for_tmux(input, 1);
        // Should be: \x1bPtmux; + (inner with doubled ESC) + \x1b\\
        assert!(result.starts_with(b"\x1bPtmux;"));
        assert!(result.ends_with(b"\x1b\\"));
        // Inner should have doubled ESCs: \x1b\x1b_G ... \x1b\x1b\\
        // Total ESC count: 2 original ESCs doubled = 4, plus 2 for wrapper = 6
        let esc_count = result.iter().filter(|&&b| b == 0x1b).count();
        assert_eq!(esc_count, 6);
    }

    #[test]
    fn wrap_for_tmux_depth_2_doubles_again() {
        let input = b"\x1b_Ga=T;data\x1b\\";
        let result = wrap_for_tmux(input, 2);
        // Depth 2: outer wrapper adds its own ESCs, inner ESCs are doubled twice
        assert!(result.starts_with(b"\x1bPtmux;"));
        assert!(result.ends_with(b"\x1b\\"));
        // 2 original ESCs → depth 1: 4 inner + 2 wrapper = 6
        // depth 2: 6 inner ESCs doubled = 12 + 2 wrapper = 14
        let esc_count = result.iter().filter(|&&b| b == 0x1b).count();
        assert_eq!(esc_count, 14);
    }

    #[test]
    fn extract_socket_parses_tmux_format() {
        assert_eq!(extract_socket("/tmp/tmux-501/default,12345,0"), Some("/tmp/tmux-501/default"));
        assert_eq!(extract_socket(""), None);
        assert_eq!(extract_socket(",12345,0"), None);
    }
}
