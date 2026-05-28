# anki-tui

Terminal-based flashcard reviewer for local Anki collections. Read and review your cards without leaving the terminal.

## Features

- **Deck browser** with collapsible hierarchy and due counts (new/learn/review)
- **Card review** with front/back flow and Anki's official scheduler
- **HTML rendering** to terminal: bold, italic, colors, lists, tables, `<hr>`
- **Inline images** via Kitty graphics protocol (Kitty, WezTerm, Ghostty)
- **Audio playback** for `[sound:]` references (MP3, OGG) with auto-play
- **Write-back through Anki's backend** — reviews sync with Anki desktop/mobile
- **Dry-run mode** for safe browsing without modifying the collection
- **Sub-deck gathering** — studying a parent deck includes all children

## Install

Requires Rust 1.85+ and [`uv`](https://docs.astral.sh/uv/) for the Python sidecar.

```bash
git clone <repo-url>
cd anki-tui
cargo build --release
```

Binary at `target/release/anki-tui`.

The first run starts the sidecar with `uv --project sidecar run ...`, which resolves the
official `anki` Python package into `sidecar/.venv`.

## Usage

```bash
# Auto-detect Anki collection
anki-tui

# Use a specific collection (e.g. a copy for testing)
anki-tui --collection path/to/collection.anki2

# Override media directory (if collection is a copy)
anki-tui --collection copy.anki2 --media-dir ~/Library/Application\ Support/Anki2/User\ 1/collection.media

# Browse without writing to the database
anki-tui --dry-run
```

## Keybindings

### Deck Selection

| Key | Action |
|-----|--------|
| `j` / `k` / arrows | Navigate |
| `Tab` / `l` / right | Expand/collapse deck |
| `h` / left | Collapse deck |
| `Enter` | Study selected deck |
| `q` | Quit |

### Card Review

| Key | Action |
|-----|--------|
| `Space` / `Enter` | Show answer |
| `1` | Again |
| `2` | Hard |
| `3` / `Space` | Good |
| `4` | Easy |
| `j` / `k` | Scroll card content |
| `r` | Replay audio |
| `q` | Back to deck list |

## How It Works

- Starts a `uv`-managed Python sidecar next to the Rust TUI
- The sidecar opens the collection with Anki's official Python package
- Deck counts, rendered card HTML, interval labels, and answers come from Anki's scheduler
- Converts HTML to styled terminal text via `scraper` + `ratatui`
- Live answers are written through Anki's backend; `--dry-run` skips writes in-memory

## Limitations

- Requires a working `uv` installation and a compatible official `anki` Python package
- Do not keep Anki Desktop open on the same collection while using live review mode
- `--dry-run` is a no-write preview; it advances through queued cards for the session but does not simulate all future scheduler transitions
- Undo is not implemented in the TUI

## Dependencies

ratatui, crossterm, scraper, rodio, image, base64, thiserror, serde, serde_json, dirs, plus the `uv` sidecar dependencies in `sidecar/pyproject.toml`
