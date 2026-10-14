#!/usr/bin/env bash
# FirnCam photo core measurement. Usage: tools/photo/run.sh [outdir]  (default: a temp dir that is removed)
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
D="${1:-$(mktemp -d)}"
[ -z "$1" ] && trap 'rm -rf "$D"' EXIT
mkdir -p "$D"
FIRNLIB="$ROOT/lib" "$FIRNC" --opt-level=release-fast -o "$D/eval" "$ROOT/tools/photo/eval.fi"
( cd "$D" && ./eval | tee metrics.txt )
python3 -I "$ROOT/tools/photo/crosscheck.py" "$D"
