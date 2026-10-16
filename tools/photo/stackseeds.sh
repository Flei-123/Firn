#!/usr/bin/env bash
# FirnCam stack over seeds 5..12: stack vs best single frame. Usage: tools/photo/stackseeds.sh [outfile]
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
D="$(mktemp -d)"
trap 'rm -rf "$D"' EXIT
FIRNLIB="$ROOT/lib" "$FIRNC" --opt-level=release-fast -o "$D/stackseeds" "$ROOT/tools/photo/stackseeds.fi"
if [ -n "$1" ]; then "$D/stackseeds" > "$1"; else "$D/stackseeds"; fi
