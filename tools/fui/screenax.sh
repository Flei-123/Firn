#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
set -u
cd "$(dirname "$0")/../.." || exit 2
FIRNC="${FIRNC:-firnc}"
for t in dbus-daemon Xvfb xdotool; do command -v $t >/dev/null 2>&1 || { echo "screenax: no $t -- not testable"; exit 77; }; done
python3 -c 'import dbus' 2>/dev/null || { echo "screenax: python3 dbus missing -- not testable"; exit 77; }
D="$(mktemp -d)"
DPID=""; APID=""; XPID=""
trap 'kill "$DPID" "$APID" "$XPID" 2>/dev/null; rm -rf "$D"' EXIT
FIRNLIB="$PWD/lib" "$FIRNC" --opt-level=dev -o "$D/settings" examples/fui_apps/settings/main.fi || { echo "screenax: build failed"; exit 1; }
"$D/settings" --theme dark --page 2 --shot "$D/s.png" --ax "$D/tree.json" || { echo "screenax: oracle failed"; exit 1; }
dbus-daemon --session --nofork --address="unix:path=$D/bus" --print-pid=3 3>"$D/pid" >/dev/null 2>&1 &
i=0
while [ ! -S "$D/bus" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
DPID="$(cat "$D/pid" 2>/dev/null)"
[ -S "$D/bus" ] || { echo "screenax: private bus did not start"; exit 77; }
Xvfb -displayfd 3 -screen 0 1280x800x24 3>"$D/disp" >/dev/null 2>&1 &
XPID=$!
i=0
while [ ! -s "$D/disp" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
DISPLAY=":$(cat "$D/disp")" FUI_ATSPI="unix:path=$D/bus" "$D/settings" --theme dark --page 2 >"$D/app.log" 2>&1 &
APID=$!
sleep 3
python3 -I tools/fui/screenax_test.py "unix:path=$D/bus" "$D/tree.json" Settings
R=$?
DISPLAY=":$(cat "$D/disp")" xdotool search --name Settings windowfocus key Tab key Tab 2>/dev/null
sleep 1
kill -0 "$APID" 2>/dev/null || { echo "  WRONG the app died on Tab"; R=1; }
exit $R
