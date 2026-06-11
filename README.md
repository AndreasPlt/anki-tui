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

Requires Rust 1.85+ to build and Python 3.10-3.13 at runtime for the Python sidecar.
`uv` is only used for sidecar development tests; the released TUI does not require it.

```bash
git clone <repo-url>
cd anki-tui
cargo build --release
```

Binary at `target/release/anki-tui`.

The sidecar installs Anki's official Python package from a local wheelhouse on first run.
For development or packaging, create that wheelhouse with:

```bash
scripts/build-sidecar-wheelhouse.sh
```

The script chooses Python 3.13, 3.12, 3.11, or 3.10 in the same order as the TUI. To force a
specific supported interpreter, run `PYTHON=/path/to/python3.13 scripts/build-sidecar-wheelhouse.sh`.

By default the app looks for wheels in `$XDG_DATA_HOME/anki-tui/wheels`, then
`~/.local/share/anki-tui/wheels`, then an install-relative `share/anki-tui/wheels`, and finally
`sidecar/wheels` in the source tree. Set `ANKI_TUI_WHEELHOUSE_DIR` to override this.

The first run creates a managed virtual environment under `$XDG_DATA_HOME/anki-tui/sidecar` or,
if `XDG_DATA_HOME` is unset, `~/.local/share/anki-tui/sidecar`. Set
`ANKI_TUI_SIDECAR_HOME` to override this.

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

# Resume the last review session (or set ANKI_TUI_RESUME=1)
anki-tui --resume
```

With `--resume`, the current deck is remembered in `~/.local/share/anki-tui/session.json`
(respects `XDG_DATA_HOME`) and the next `--resume` run jumps straight back into reviewing it,
skipping the deck list. The session is cleared once the deck is finished. Sessions are only
recorded on runs started with `--resume`.

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

- Starts a managed Python sidecar environment from bundled wheels
- The sidecar opens the collection with Anki's official Python package
- Deck counts, rendered card HTML, interval labels, and answers come from Anki's scheduler
- Converts HTML to styled terminal text via `scraper` + `ratatui`
- Live answers are written through Anki's backend; `--dry-run` skips writes in-memory

## Limitations

- Requires Python 3.10-3.13 and an offline wheelhouse containing the compatible official `anki` Python package
- Do not keep Anki Desktop open on the same collection while using live review mode
- `--dry-run` is a no-write preview; it advances through queued cards for the session but does not simulate all future scheduler transitions
- Undo is not implemented in the TUI

## Dependencies

ratatui, crossterm, scraper, rodio, image, base64, thiserror, serde, serde_json, dirs, plus the sidecar wheels generated from `sidecar/pyproject.toml` / `anki==25.9.4`.

Useful sidecar overrides:

- `ANKI_TUI_SIDECAR_CMD`: run a fully custom sidecar command
- `ANKI_TUI_PYTHON`: choose the Python executable used to create the managed venv
- `ANKI_TUI_SIDECAR_HOME`: choose where the managed venv and sidecar script are stored
- `ANKI_TUI_WHEELHOUSE_DIR`: choose where offline wheels are read from

## Disclaimer

Almost entirely with AI (Claude Code) — partly out of curiosity to see how far a
"purely vibe-coded" project can go, partly to quickly validate a random idea I had while working on my dissertation.
I'm surprisingly satisfied with the result, and hope this can be useful to someone out there as well.
