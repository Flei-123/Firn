#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/input/win.sh -- lib/input on the Windows target: tools/input/
# win_check.fi built for x86_64-windows (backend: SendInput) and run under
# Wine on a virtual X display. GetCursorPos has to see the pointer move;
# every other event is checked by SendInput's count of injected records.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
for t in x86_64-w64-mingw32-as x86_64-w64-mingw32-ld wine xvfb-run; do
    command -v "$t" >/dev/null 2>&1 || { echo "SKIP: $t is missing"; exit 0; }
done
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
"$FIRNC" --target=x86_64-windows -o "$W/check.exe" tools/input/win_check.fi 2> "$W/b.log" || { cat "$W/b.log"; exit 1; }
export WINEDEBUG=-all WINEPREFIX="${WINEPREFIX:-${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn}"
# The pointer is read back from a virtual X display; under load (or with a
# second Wine on the same prefix) the first move can be read too early.
# Three attempts, the log of the LAST one is shown -- a real defect fails all three.
for attempt in 1 2 3; do
    timeout 120 xvfb-run -a wine "$W/check.exe" > "$W/out.log" 2>&1
    RC=$?
    grep -q '^ALL-OK' "$W/out.log" && [ "$RC" -eq 0 ] && break
    [ "$attempt" -lt 3 ] && { echo "attempt $attempt failed, retrying"; sleep 2; }
done
cat "$W/out.log"
grep -q '^ALL-OK' "$W/out.log" && [ "$RC" -eq 0 ] && { echo "INPUT-WIN OK ($(grep -c '^OK' "$W/out.log") checks, SendInput under Wine)"; exit 0; }
echo "INPUT-WIN FAIL"
exit 1
