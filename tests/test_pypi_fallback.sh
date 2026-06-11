#!/bin/bash
# End-to-end test of the PyPI wheelhouse fallback: with no wheelhouse visible,
# the first run must download anki from PyPI into a throwaway sidecar home.
#
# Requires network and downloads the anki package (large, one-time per run).
# Temporarily moves sidecar/wheels aside (restored on exit).
# Usage: bash tests/test_pypi_fallback.sh
set -euo pipefail

cd "$(dirname "$0")/.."

cargo build --release

TMP=$(mktemp -d)
WHEELS_BACKUP=""
cleanup() {
    if [ -n "$WHEELS_BACKUP" ] && [ -d "$WHEELS_BACKUP" ]; then
        mv "$WHEELS_BACKUP" sidecar/wheels
    fi
    rm -rf "$TMP"
}
trap cleanup EXIT

# Hide every wheelhouse candidate: XDG/HOME point into the temp dir, and the
# source-tree wheelhouse (baked in via CARGO_MANIFEST_DIR) is moved aside.
if [ -d sidecar/wheels ]; then
    WHEELS_BACKUP="$TMP/wheels-backup"
    mv sidecar/wheels "$WHEELS_BACKUP"
fi
export HOME="$TMP/home"
export XDG_DATA_HOME="$TMP/data"
export ANKI_TUI_SIDECAR_HOME="$TMP/sidecar-home"
mkdir -p "$HOME"

# macOS has no GNU `timeout`; perl's alarm is a portable stand-in.
with_timeout() {
    if command -v timeout >/dev/null 2>&1; then
        timeout "$@"
    else
        perl -e 'alarm shift; exec @ARGV' "$@"
    fi
}

# Venv setup happens before terminal init, so a non-tty run still exercises
# the full bootstrap; the TUI itself then exits on the failed event read.
with_timeout 900 ./target/release/anki-tui \
    --collection "$TMP/collection.anki2" --dry-run \
    </dev/null >/dev/null 2>"$TMP/stderr.log" || true

echo "--- stderr ---"
cat "$TMP/stderr.log"
echo "--------------"

grep -q "no local wheelhouse found" "$TMP/stderr.log" \
    || { echo "FAIL: PyPI fallback was not triggered"; exit 1; }

VENV_PYTHON=$(echo "$ANKI_TUI_SIDECAR_HOME"/venvs/*/bin/python)
[ -x "$VENV_PYTHON" ] || { echo "FAIL: managed venv was not created"; exit 1; }

"$VENV_PYTHON" -I -c "import anki.collection, anki.buildinfo; print('anki', anki.buildinfo.version)" \
    || { echo "FAIL: anki not importable from managed venv"; exit 1; }

echo '{"id":1,"method":"shutdown","params":{}}' \
    | "$VENV_PYTHON" -I -u "$ANKI_TUI_SIDECAR_HOME/anki_tui_sidecar.py" \
    | grep -q '"shutdown": *true' \
    || { echo "FAIL: deployed sidecar script did not respond"; exit 1; }

echo "PASS: PyPI fallback installed anki and the sidecar responds"
