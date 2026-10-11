#!/usr/bin/env bash
# tools/audio/run.sh -- audio.synth + audio.mixer bit for bit against Python.
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
FIRNLIB="$(pwd)/lib" "$FIRNC" -o "$W/mixcheck" tools/audio/mixcheck_main.fi
python3 tools/audio/check.py "$W/mixcheck"
