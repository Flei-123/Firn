#!/usr/bin/env bash
# tools/pystr_cross/run.sh -- std.pystr and regex.pyre against Python str / re (random cases + fuzz).
# Usage: bash tools/pystr_cross/run.sh [SEED] [CASES]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
SEED=${1:-20261017}
CASES=${2:-400}
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT
for L in no-opt dev-fast release-fast release-safe; do
    FL="--opt-level=$L"; [ "$L" = no-opt ] && FL="--no-opt"
    "$FIRNC" $FL -o "$TMPD/pystr_tool" tools/pystr_cross/pystr_tool.fi || { echo "FAIL  build $L"; exit 1; }
    python3 tools/pystr_cross/cross.py "$TMPD/pystr_tool" "$TMPD/work" "$SEED" "$CASES" || { echo "FAIL  $L"; exit 1; }
done
echo "pystr_cross: ok"
