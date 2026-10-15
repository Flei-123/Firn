#!/usr/bin/env bash
# FirnCam focus stack measurement on the synthetic TV scene. Usage: tools/photo/stackeval.sh [outdir]
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
D="${1:-$(mktemp -d)}"
[ -z "$1" ] && trap 'rm -rf "$D"' EXIT
mkdir -p "$D"
FIRNLIB="$ROOT/lib" "$FIRNC" --opt-level=release-fast -o "$D/stackeval" "$ROOT/tools/photo/stackeval.fi"
( cd "$D" && ./stackeval | tee metrics.txt )
