#!/usr/bin/env bash
# tools/input/pad_run.sh -- gamepads + evdev raw input (docs/INPUT.md):
#   tests/2794 padcore (evdev/joydev bytes, SDL mapping lines, deadzone, XInput, 4000 random events vs a model)
#   tests/2795 padsys  (the real read(2) path on a pipe with fake input_event / js_event bytes, unplug, SDL database)
#   tests/2797 rawdev  (keyboard / mouse from evdev bytes through the raw state machine, 2000 random frames)
#   tests/2866 rumble (padff bytes: ff_effect 48 octets / input_event / ioctl numbers; padsys call order through the test tap)
#   gcc check (if <linux/input.h> is there): sizeof/offsetof of struct ff_effect and EVIOCSFF/EVIOCRMFF equal padff's
#   Windows builds of the pad front door and the raw device stub (compile only)
# Usage: bash tools/input/pad_run.sh [firnc]
set -u
cd "$(dirname "$0")/../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
fail=0
for t in 2794_input_padcore 2795_input_padsys 2797_input_rawdev 2866_input_rumble; do
    if "$FIRNC" -o "$D/t" "tests/$t.fi" 2>"$D/err" && "$D/t"; then echo "   ok:   tests/$t"
    else echo "   FAIL: tests/$t (exit $?)"; head -5 "$D/err"; fail=1; fi
done
if command -v gcc >/dev/null && [ -f /usr/include/linux/input.h ]; then
    cat > "$D/ff.c" <<'C'
#include <stdio.h>
#include <stddef.h>
#include <linux/input.h>
int main(void) {
    printf("%zu %zu %zu %zu %zu %zu %zu %lu %lu %lu %zu\n", sizeof(struct ff_effect), offsetof(struct ff_effect, id), offsetof(struct ff_effect, replay),
        offsetof(struct ff_effect, u), offsetof(struct ff_effect, u.rumble.weak_magnitude), sizeof(struct input_event), offsetof(struct input_event, value),
        (unsigned long)EVIOCSFF, (unsigned long)EVIOCRMFF, (unsigned long)EVIOCGBIT(EV_FF, 16), offsetof(struct input_event, code));
    return 0;
}
C
    if gcc -o "$D/ff" "$D/ff.c" 2>"$D/err" && [ "$("$D/ff")" = "48 2 10 16 18 24 20 1076905344 1074021761 2148549941 18" ]; then echo "   ok:   gcc: struct ff_effect (48 octets, id@2, replay@10, union@16, weak@18), input_event 24, EVIOCSFF / EVIOCRMFF / EVIOCGBIT(EV_FF) equal padff.fi"
    else echo "   FAIL: gcc struct check: $("$D/ff" 2>&1 | head -2)"; head -3 "$D/err"; fail=1; fi
else echo "   SKIP: gcc or <linux/input.h> missing (struct layout not cross-checked)"; fi
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
    pad.pad_rumble(&s, 0, 1.0, 0.5, 100 as u32)
    pad.pad_rumble_gain(&s, 0, 0.5)
    pad.pad_rumble_stop(&s, 0)
    pad.pad_close(&s)
    var d: rawdev.RawDev = rawdev.rawdev_new()
    return rawdev.rawdev_open(&d) as i32
}
FI
if "$FIRNC" --target=x86_64-windows -o "$D/win.exe" "$D/win.fi" 2>"$D/err"; then echo "   ok:   pad (XInput + XInputSetState rumble via dynlib) + rawdev stub build for Windows (NOT run)"
else echo "   FAIL: Windows build"; head -5 "$D/err"; fail=1; fi
[ "$fail" -eq 0 ] && echo "RESULT pad: ok" || echo "RESULT pad: FAIL"
exit "$fail"
