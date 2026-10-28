#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-/root/firn/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
D=$(mktemp -d)
XPID=""
trap 'kill $XPID $(jobs -p) 2>/dev/null; rm -rf "$D"' EXIT
"$FIRNC" --no-opt -o "$D/focus" tools/input/focus_main.fi 2> "$D/build.log" || { cat "$D/build.log"; exit 1; }
DISP=":${FOCUS_DISPLAY:-98}"
Xvfb "$DISP" -screen 0 800x600x24 > /dev/null 2>&1 &
XPID=$!
export DISPLAY="$DISP"
sleep 1
fail=0
check() { if ! grep -q "$2" "$1"; then echo "FAIL: $3"; fail=1; else echo "ok: $3"; fi; }
for mode in raw warp; do
    "$D/focus" "$mode" > "$D/a.$mode.log" 2>&1 &
    APID=$!
    for i in $(seq 1 50); do grep -q READY "$D/a.$mode.log" 2>/dev/null && break; sleep 0.2; done
    "$D/focus" raw plain > "$D/b.$mode.log" 2>&1 &
    BPID=$!
    for i in $(seq 1 50); do grep -q READY "$D/b.$mode.log" 2>/dev/null && break; sleep 0.2; done
    WA=$(xdotool search --name mffocus_a | head -1)
    WB=$(xdotool search --name mffocus_b | head -1)
    xdotool windowfocus "$WA"; sleep 0.5
    xdotool windowfocus "$WB"; sleep 1
    xdotool mousemove 700 500; sleep 0.8
    POS=$(xdotool getmouselocation | cut -d' ' -f1,2)
    xdotool windowfocus "$WA"; sleep 1
    xdotool key --window "$WA" q 2>/dev/null; xdotool key --window "$WB" q 2>/dev/null; sleep 0.5
    wait $APID $BPID 2>/dev/null
    check "$D/a.$mode.log" "^GRAB 1" "$mode: initial grab"
    check "$D/a.$mode.log" "^FOCUS 0 captured_after=0" "$mode: focus loss releases grab"
    check "$D/a.$mode.log" "^REGRAB focus=0 ok=0" "$mode: no regrab while unfocused"
    check "$D/a.$mode.log" "^REGRAB focus=1 ok=1" "$mode: regrab allowed after focus return"
    if [ "$POS" = "x:700 y:500" ]; then echo "ok: $mode: pointer stays at 700,500 while unfocused"; else echo "FAIL: $mode: pointer moved to $POS"; fail=1; fi
done
[ $fail = 0 ] && echo "FOCUS OK" || echo "FOCUS FAILED"
exit $fail
