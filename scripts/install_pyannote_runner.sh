#!/usr/bin/env bash
#
# Drop the pyannote_runner script into ~/.local/bin so it is on $PATH.
#
# This script does *not* install pyannote.audio itself — `furu setup` checks
# for the import separately and prints an install hint if it is missing.
# The user can install pyannote.audio with whichever Python tool they
# prefer (pipx, pip --user, a venv, or a system package).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUNNER_SRC="$SCRIPT_DIR/pyannote_runner"
TARGET_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
TARGET="$TARGET_DIR/pyannote_runner"

if [[ ! -f "$RUNNER_SRC" ]]; then
    echo "error: $RUNNER_SRC not found" >&2
    exit 1
fi

mkdir -p "$TARGET_DIR"
install -m 0755 "$RUNNER_SRC" "$TARGET"

echo "pyannote_runner installed to $TARGET"

case ":$PATH:" in
    *":$TARGET_DIR:"*) ;;
    *) echo "warning: $TARGET_DIR is not on \$PATH — add it to use 'pyannote_runner'" ;;
esac
