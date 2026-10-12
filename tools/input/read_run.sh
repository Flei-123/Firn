#!/usr/bin/env bash
# tools/input/read_run.sh -- lib/input (docs/INPUT.md):
#   1. tests/2792_input_raw.fi: the key / mouse state machine (scripted + 2000 random frames vs a model)
#   2. the relative mouse grab on a private Xvfb, driven by xdotool (XTest): XInput2 raw motion AND the
#      warp-to-centre fallback must both sum 5 x (10,-4) + 3 x (-7,20) = (29,40); raw keys: scancode + keysym
#   3. the same grab program built for Windows (compile only; Wine ran into MAKE-FAILED here)
# Prints SKIP (exit 0) when Xvfb / xdotool are missing. Usage: bash tools/input/read_run.sh [firnc]
set -u
cd "$(dirname "$0")/../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
D=$(mktemp -d)
XPID=""
cleanup() { [ -n "$XPID" ] && kill "$XPID" 2>/dev/null; rm -rf "$D" 2>/dev/null || { sleep 2; rm -rf "$D"; }; }
trap cleanup EXIT
fail=0
"$FIRNC" -o "$D/raw" tests/2792_input_raw.fi || { echo "   FAIL: compile 2792"; exit 1; }
if "$D/raw"; then echo "   ok:   raw.fi state machine (scripted + 2000 random frames)"; else echo "   FAIL: raw.fi state machine exit $?"; fail=1; fi
if ! command -v Xvfb >/dev/null 2>&1 || ! command -v xdotool >/dev/null 2>&1; then
    echo "   SKIP: grab test (Xvfb or xdotool missing)"
    [ "$fail" -eq 0 ] && echo "RESULT input: ok" || echo "RESULT input: FAIL"
    exit "$fail"
fi
"$FIRNC" --opt-level=dev -o "$D/grab" tools/input/grab_main.fi || { echo "   FAIL: compile grab_main.fi"; exit 1; }
mkfifo "$D/disp" && { Xvfb -displayfd 3 -screen 0 800x600x24 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 & XPID=$!; }
read -r -t 20 n <"$D/disp" || n=""
[ -n "$n" ] || { echo "   SKIP: grab test (Xvfb did not start)"; exit 0; }
export DISPLAY=":$n"
for mode in raw warp; do
    "$D/grab" $mode > "$D/out.$mode" 2>&1 &
    GP=$!
    for i in $(seq 1 50); do grep -q READY "$D/out.$mode" 2>/dev/null && break; sleep 0.2; done
    xdotool mousemove 200 150; sleep 0.2
    for i in 1 2 3 4 5; do xdotool mousemove_relative -- 10 -4; sleep 0.05; done
    for i in 1 2 3; do xdotool mousemove_relative -- -7 20; sleep 0.05; done
    sleep 0.3; xdotool key a; xdotool key q
    wait $GP
    want_raw=1; [ "$mode" = warp ] && want_raw=0
    if grep -q "^GRAB 1 raw=$want_raw" "$D/out.$mode" && grep -q "^SUM 29 40 motion_events=0" "$D/out.$mode" \
       && grep -q "^KEY 1 sc=30 sym=97" "$D/out.$mode" && grep -q "^KEY 0 sc=30 sym=97" "$D/out.$mode"; then
        echo "   ok:   mouse grab $mode: delta sum (29,40), no EV_MOTION, raw key a: scancode 30 keysym 97"
    else
        echo "   FAIL: mouse grab $mode"; sed 's/^/      /' "$D/out.$mode"; fail=1
    fi
done
if "$FIRNC" --target=x86_64-windows -o "$D/grab.exe" tools/input/grab_main.fi 2>"$D/err"; then
    echo "   ok:   grab program builds for Windows (RawInput / ClipCursor path compiled; NOT run)"
else echo "   FAIL: grab_main.fi does not build for Windows"; head -5 "$D/err"; fail=1; fi
[ "$fail" -eq 0 ] && echo "RESULT input: ok" || echo "RESULT input: FAIL"
exit "$fail"
