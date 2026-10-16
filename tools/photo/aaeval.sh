#!/usr/bin/env bash
# FirnCam anti-alias measurement (r658). Usage: tools/photo/aaeval.sh [outdir]   (default: a temp dir that is removed)
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
LEVEL="${LEVEL:-release-fast}"
D="${1:-$(mktemp -d)}"
[ -z "$1" ] && trap 'rm -rf "$D"' EXIT
mkdir -p "$D"
FIRNLIB="$ROOT/lib" "$FIRNC" --opt-level="$LEVEL" -o "$D/aaeval" "$ROOT/tools/photo/aaeval.fi"
( cd "$D" && ./aaeval )
