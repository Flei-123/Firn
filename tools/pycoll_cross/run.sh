#!/usr/bin/env bash
# tools/pycoll_cross/run.sh -- std.pycoll (stable sorts, group_by, count_by, dict_items_sorted) against Python.
# Usage: bash tools/pycoll_cross/run.sh [SEED] [CASES]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
SEED=${1:-20261018}
CASES=${2:-300}
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT
for L in dev-fast release-safe; do
    "$FIRNC" --opt-level=$L -o "$TMPD/pycoll_tool" tools/pycoll_cross/pycoll_tool.fi || { echo "FAIL  build $L"; exit 1; }
    python3 tools/pycoll_cross/cross.py "$TMPD/pycoll_tool" "$TMPD/work" "$SEED" "$CASES" || { echo "FAIL  $L"; exit 1; }
done
echo "pycoll_cross: ok"
