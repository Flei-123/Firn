#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/window/extras_wine.sh -- the Win32 half of lib/window/extras.fi (topmost, hide/show, frameless, work area,
# place_corner, fit_height, hotkey register) built for x86_64-windows and run under Wine on a private Xvfb.
# Output goes to files (never read Wine through a pipe). Own WINEPREFIX; `wineserver -k` at the end.
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-/root/firn/compiler/target/release/firnc}"
for t in wine Xvfb; do command -v $t > /dev/null || { echo "SKIP: $t missing"; exit 0; }; done
D=$(mktemp -d)
export WINEPREFIX="$D/prefix" WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
cleanup() { wineserver -k 2> /dev/null; kill "${XPID:-0}" 2> /dev/null; rm -rf "$D"; }
trap cleanup EXIT
"$FIRNC" --target=x86_64-windows -o "$D/probe.exe" tools/window/extras_wine_probe.fi > "$D/build.log" 2>&1 || { grep -v RWX "$D/build.log" | head; echo FAIL build; exit 1; }
Xvfb -displayfd 3 -screen 0 1024x768x24 -nolisten tcp 3> "$D/disp" > /dev/null 2>&1 &
XPID=$!
for _ in $(seq 1 100); do [ -s "$D/disp" ] && break; sleep 0.1; done
export DISPLAY=":$(head -1 "$D/disp")"
timeout 120 wine "$D/probe.exe" > "$D/out" 2> "$D/err"
rc=$?
cat "$D/err" | grep -v -E "^$" | head -20
echo "wine rc=$rc"
exit $rc
