#!/bin/bash
# SPDX-License-Identifier: MPL-2.0
# tools/fui/rechook.sh -- FUI_RECORD hook of the apphost on a private Xvfb: operate the window with xdotool, check the log.
# Needs FIRNC (firnc binary), Xvfb, xdotool; SKIP (exit 0) without them.
set -u
HERE=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-firnc}
command -v Xvfb >/dev/null && command -v xdotool >/dev/null || { echo "rechook SKIP (no Xvfb/xdotool)"; exit 0; }
W=$(mktemp -d); XP=0
trap '[ "$XP" != 0 ] && kill $XP 2>/dev/null; rm -rf "$W"' EXIT
FIRNLIB=$HERE/lib $FIRNC --opt-level=dev -o $W/prog $HERE/tools/fui/rechook_main.fi || { echo "rechook FAILED (build)"; exit 1; }
export XDG_RUNTIME_DIR=$W/xdg; mkdir -m 700 $XDG_RUNTIME_DIR
DN=$((60 + $$ % 30))
Xvfb :$DN -screen 0 800x600x24 >/dev/null 2>&1 & XP=$!
sleep 1; export DISPLAY=:$DN
FUI_RECORD=$W/x.rec timeout 30 $W/prog >$W/prog.log 2>&1 & PP=$!
sleep 3
xdotool mousemove 100 80; sleep 0.2; xdotool mousemove 120 90; xdotool click 1
xdotool type --delay 50 ab; sleep 0.3
xdotool key Tab; xdotool key Return; sleep 1
if kill -0 $PP 2>/dev/null; then xdotool key Tab; xdotool key space; sleep 1; fi
wait $PP; rc=$?
err=0
[ -s $W/x.rec ] || { echo "WRONG: no log written (rc=$rc)"; err=1; }
for k in M D U K T; do grep -q "^$k " $W/x.rec 2>/dev/null || { echo "WRONG: no '$k' lines"; err=1; }; done
grep -c . $W/x.rec 2>/dev/null | sed 's/^/log lines: /'
[ $err = 0 ] && echo "RECHOOK PASSED" || { echo "RECHOOK FAILED"; head -30 $W/x.rec; cat $W/prog.log | head; exit 1; }
