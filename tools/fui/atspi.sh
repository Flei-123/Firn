#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/atspi.sh -- the AT-SPI bridge end to end on a PRIVATE dbus-daemon (no registry, no session bus).
#   FIRNC=... sh tools/fui/atspi.sh      builds tools/fui/atspi_main.fi, serves the tree, runs atspi_test.py
# Exit 0 = every check passed; 77 = the prerequisites are missing (dbus-daemon, python3 dbus).
set -u
cd "$(dirname "$0")/../.." || exit 2
FIRNC="${FIRNC:-firnc}"
command -v dbus-daemon >/dev/null 2>&1 || { echo "atspi: no dbus-daemon -- not testable"; exit 77; }
python3 -c 'import dbus' 2>/dev/null || { echo "atspi: python3 dbus missing -- not testable"; exit 77; }
D="$(mktemp -d)"
trap 'kill "$DPID" 2>/dev/null; kill "$APID" 2>/dev/null; kill "$XPID" 2>/dev/null; rm -rf "$D"' EXIT
DPID=""; APID=""; XPID=""
FIRNLIB="$PWD/lib" "$FIRNC" --opt-level=dev -o "$D/atspi" tools/fui/atspi_main.fi || { echo "atspi: build failed"; exit 1; }
dbus-daemon --session --nofork --address="unix:path=$D/bus" --print-pid=3 3>"$D/pid" >/dev/null 2>&1 &
i=0
while [ ! -S "$D/bus" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
DPID="$(cat "$D/pid" 2>/dev/null)"
[ -S "$D/bus" ] || { echo "atspi: private bus did not start"; exit 77; }
FUI_ATSPI_ADDR="unix:path=$D/bus" FUI_ATSPI_JSON="$D/tree.json" "$D/atspi" >"$D/app.log" 2>&1 &
APID=$!
i=0
while ! grep -q "ATSPI READY" "$D/app.log" 2>/dev/null && [ $i -lt 100 ]; do sleep 0.1; i=$((i + 1)); done
grep -q "ATSPI READY" "$D/app.log" || { echo "atspi: the program did not come up"; cat "$D/app.log"; exit 1; }
head -n 1 "$D/app.log"
python3 -I tools/fui/atspi_test.py "unix:path=$D/bus" "$D/tree.json"
R=$?
wait "$APID" 2>/dev/null
tail -n 2 "$D/app.log"
[ $R -eq 0 ] || exit $R
# --- phase 2: the same tree in a REAL X11 window (own Xvfb), served from the window loop,
# on the same private bus (the first program has left it)
command -v Xvfb >/dev/null 2>&1 || { echo "atspi: no Xvfb -- the live window phase is not testable"; exit 0; }
FIRNLIB="$PWD/lib" "$FIRNC" --opt-level=dev -o "$D/atspilive" tools/fui/atspilive_main.fi || { echo "atspi: live build failed"; exit 1; }
Xvfb -displayfd 3 -screen 0 1280x800x24 3>"$D/disp" >/dev/null 2>&1 &
XPID=$!
i=0
while [ ! -s "$D/disp" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
[ -s "$D/disp" ] || { echo "atspi: Xvfb did not start -- live phase not testable"; exit 0; }
DISPLAY=":$(cat "$D/disp")" FUI_ATSPI="unix:path=$D/bus" "$D/atspilive" >"$D/live.log" 2>&1 &
APID=$!
i=0
while ! grep -q "ATSPI LIVE" "$D/live.log" 2>/dev/null && [ $i -lt 100 ]; do sleep 0.1; i=$((i + 1)); done
sleep 1
echo "-- live window ($(head -n 1 "$D/live.log"))"
python3 -I tools/fui/atspi_test.py "unix:path=$D/bus" "$D/tree.json" --live
R=$?
i=0
while kill -0 "$APID" 2>/dev/null && [ $i -lt 30 ]; do sleep 0.1; i=$((i + 1)); done
kill -0 "$APID" 2>/dev/null && { echo "  WRONG the window did not close after the Quit action"; R=1; }
exit $R
