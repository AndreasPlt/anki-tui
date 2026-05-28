# anki-tui

Terminal-based flashcard reviewer for local Anki collections. Read and review your cards without leaving the terminal.

## Features

- **Deck browser** with collapsible hierarchy and due counts (new/learn/review)
- **Card review** with front/back flow and SM-2 scheduling
- **HTML rendering** to terminal: bold, italic, colors, lists, tables, `<hr>`
- **Inline images** via Kitty graphics protocol (Kitty, WezTerm, Ghostty)
- **Audio playback** for `[sound:]` references (MP3, OGG) with auto-play
- **Write-back** to Anki's SQLite DB — reviews sync with Anki desktop/mobile
- **Dry-run mode** for safe browsing without modifying the collection
- **Sub-deck gathering** — studying a parent deck includes all children

## Install

Requires Rust 1.85+.

```bash
git clone <repo-url>
cd anki-tui
cargo build --release
```

Binary at `target/release/anki-tui`.

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

- Reads Anki's `collection.anki2` SQLite database directly
- Decodes protobuf blobs for templates, notetypes, and deck configs
- Renders Anki's Mustache-like templates (`{{Field}}`, conditionals, cloze deletions)
- Converts HTML to styled terminal text via `scraper` + `ratatui`
- Implements SM-2 scheduling (interval/ease computation, learning steps, relearning)
- Writes card state and revlog entries back to the DB with `usn=-1` for sync compatibility

## Limitations

- **SM-2 only** — FSRS scheduling is not implemented (falls back to SM-2 fields)
- **No filtered decks, burying, or undo**
- **Prototype-level Anki compatibility** — protobuf field mappings may break with Anki version changes
- **Not a replacement for Anki** — use `--dry-run` if unsure

## Dependencies

ratatui, crossterm, rusqlite, scraper, rodio, image, chrono, base64, thiserror, rand, dirs
