#!/usr/bin/env bash
# tools/gpu/run.sh -- lib/gpu smoke test: headless EGL GL 3.3 core context (Mesa llvmpipe), clear, readback.
# Prints SKIP (exit 0) when libEGL is missing. Usage: bash tools/gpu/run.sh [firnc]
set -u
cd "$(dirname "$0")/../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
[ -e /usr/lib/x86_64-linux-gnu/libEGL.so.1 ] || { echo "   SKIP: no libEGL.so.1"; exit 0; }
"$FIRNC" -o "$D/smoke" tools/gpu/smoke.fi || { echo "   FAIL: compile"; exit 1; }
env -u DISPLAY -u EGL_PLATFORM timeout 60 "$D/smoke" | sed 's/^/   /'; rc=${PIPESTATUS[0]}
[ "$rc" -eq 0 ] && echo "   ok:   GL 3.3 core context, clear + readback" || { echo "   FAIL: smoke exit $rc"; exit 1; }
