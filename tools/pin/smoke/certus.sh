#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
REPO=$1
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
cd "$REPO" || exit 1
env -u FIRNC -u FIRNC_GC_RUNTIME bash tools/work/mk.sh release-safe "$D/js" lib/js/run_main.fi > "$D/build.log" 2>&1 || { tail -15 "$D/build.log"; exit 1; }
frame() { python3 -I -c 'import struct, sys; s = sys.argv[1].encode(); sys.stdout.buffer.write(struct.pack("<II", 0, len(s)) + s)' "$1"; }
[ "$(frame 'print(6 * 7)' | timeout 30 "$D/js" | head -1)" = 42 ] || { echo "certus smoke: js run failed"; exit 1; }
