#!/usr/bin/env bash
# tools/input/pad_run.sh -- gamepads + evdev raw input (docs/INPUT.md):
#   tests/2794 padcore (evdev/joydev bytes, SDL mapping lines, deadzone, XInput, 4000 random events vs a model)
#   tests/2795 padsys  (the real read(2) path on a pipe with fake input_event / js_event bytes, unplug, SDL database)
#   tests/2797 rawdev  (keyboard / mouse from evdev bytes through the raw state machine, 2000 random frames)
#   Windows builds of the pad front door and the raw device stub (compile only)
# Usage: bash tools/input/pad_run.sh [firnc]
set -u
cd "$(dirname "$0")/../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
fail=0
for t in 2794_input_padcore 2795_input_padsys 2797_input_rawdev; do
    if "$FIRNC" -o "$D/t" "tests/$t.fi" 2>"$D/err" && "$D/t"; then echo "   ok:   tests/$t"
    else echo "   FAIL: tests/$t (exit $?)"; head -5 "$D/err"; fail=1; fi
done
cat > "$D/win.fi" <<'FI'
import std.rt
import input.raw
import input.rawdev
import input.pad
import input.padset
fn main() -> i32 {
    var s: padset.PadSet = pad.pad_new()
    pad.pad_open(&s)
    pad.pad_poll(&s)
    pad.pad_close(&s)
    var d: rawdev.RawDev = rawdev.rawdev_new()
    return rawdev.rawdev_open(&d) as i32
}
FI
if "$FIRNC" --target=x86_64-windows -o "$D/win.exe" "$D/win.fi" 2>"$D/err"; then echo "   ok:   pad (XInput via dynlib) + rawdev stub build for Windows (NOT run)"
else echo "   FAIL: Windows build"; head -5 "$D/err"; fail=1; fi
[ "$fail" -eq 0 ] && echo "RESULT pad: ok" || echo "RESULT pad: FAIL"
exit "$fail"
