#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/atspiset.sh -- Value.CurrentValue set and EditableText over AT-SPI on a screen-mode tree (private dbus-daemon).
#   FIRNC=... sh tools/fui/atspiset.sh      Exit 0 = ok; 77 = prerequisites missing.
set -u
cd "$(dirname "$0")/../.." || exit 2
FIRNC="${FIRNC:-firnc}"
command -v dbus-daemon >/dev/null 2>&1 || { echo "atspiset: no dbus-daemon -- not testable"; exit 77; }
python3 -c 'import dbus' 2>/dev/null || { echo "atspiset: python3 dbus missing -- not testable"; exit 77; }
D="$(mktemp -d)"
trap 'kill "$DPID" 2>/dev/null; kill "$APID" 2>/dev/null; rm -rf "$D"' EXIT
DPID=""; APID=""
FIRNLIB="$PWD/lib" "$FIRNC" --opt-level=dev -o "$D/atspiset" tools/fui/atspiset_main.fi || { echo "atspiset: build failed"; exit 1; }
dbus-daemon --session --nofork --address="unix:path=$D/bus" --print-pid=3 3>"$D/pid" >/dev/null 2>&1 &
i=0
while [ ! -S "$D/bus" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
DPID="$(cat "$D/pid" 2>/dev/null)"
[ -S "$D/bus" ] || { echo "atspiset: private bus did not start"; exit 77; }
FUI_ATSPI_ADDR="unix:path=$D/bus" "$D/atspiset" >"$D/app.log" 2>&1 &
APID=$!
i=0
while ! grep -q "ATSPI READY" "$D/app.log" 2>/dev/null && [ $i -lt 100 ]; do sleep 0.1; i=$((i + 1)); done
grep -q "ATSPI READY" "$D/app.log" || { echo "atspiset: the program did not come up"; cat "$D/app.log"; exit 1; }
python3 -I tools/fui/atspiset_test.py "unix:path=$D/bus"
R=$?
wait "$APID"
A=$?
cat "$D/app.log"
[ $R -eq 0 ] && [ $A -eq 0 ] || exit 1
exit 0
