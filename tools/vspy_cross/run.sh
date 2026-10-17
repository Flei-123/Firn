#!/usr/bin/env bash
# tools/vspy_cross/run.sh -- std.dict and fsx.find against Python (dict, sorted/fnmatch/os.walk).
# Usage: bash tools/vspy_cross/run.sh [SEED] [CASES]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB="$ROOT/lib"
SEED=${1:-20261017}
CASES=${2:-300}
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT
for L in dev-fast release-safe; do
    "$FIRNC" --opt-level=$L -o "$TMPD/vspy_tool" tools/vspy_cross/vspy_tool.fi || { echo "FAIL  build $L"; exit 1; }
    python3 tools/vspy_cross/cross.py "$TMPD/vspy_tool" "$TMPD/work" "$SEED" "$CASES" || { echo "FAIL  $L"; exit 1; }
done
echo "vspy_cross: ok"
