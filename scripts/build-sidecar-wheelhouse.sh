#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
WHEELHOUSE="${1:-"$ROOT/sidecar/wheels"}"

find_python() {
    if [ "${PYTHON:-}" ]; then
        printf '%s\n' "$PYTHON"
        return
    fi

    for candidate in python3.13 python3.12 python3.11 python3.10 python3; do
        if command -v "$candidate" >/dev/null 2>&1; then
            if "$candidate" -c 'import sys; raise SystemExit(not (sys.version_info[:1] == (3,) and 10 <= sys.version_info[1] < 14))'; then
                printf '%s\n' "$candidate"
                return
            fi
        fi
    done

    printf '%s\n' "Could not find Python >=3.10,<3.14. Set PYTHON=/path/to/python." >&2
    exit 1
}

PYTHON="$(find_python)"

"$PYTHON" -c 'import sys; raise SystemExit(not (sys.version_info[:1] == (3,) and 10 <= sys.version_info[1] < 14))' || {
    "$PYTHON" --version >&2
    printf '%s\n' "The sidecar requires Python >=3.10,<3.14." >&2
    exit 1
}

mkdir -p "$WHEELHOUSE"
"$PYTHON" -m pip download --only-binary=:all: --dest "$WHEELHOUSE" "anki==25.9.4"

printf 'Sidecar wheelhouse written to %s using %s\n' "$WHEELHOUSE" "$("$PYTHON" --version)"
