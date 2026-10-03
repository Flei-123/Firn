#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/keyboard_check.sh -- THE ON-SCREEN KEYBOARD TYPES INTO A fUi
# TEXT FIELD (roadmap r114), on an emulator (adb root not needed).
#
# examples/fui/form.fi as an APK (lib/@android/fui/apphost.fi). The program
# asks for the keyboard when a text field takes the focus
# (lib/android/inputmethod.fi builds an InputConnection out of dex classes
# the first time). The script
#   * taps a field and waits until the system says the keyboard is shown
#     (`dumpsys input_method`: mInputShown=true)
#   * types "hello" by TAPPING THE KEYS of the system keyboard, deletes one
#     letter with its Backspace key, switches to the second field and types
#     there -- and compares each field's pixels with what the same text looks
#     like when it arrives some other way (`input text`, i.e. key events):
#     the same pixels = the same text in the same place
#   * taps a button: the keyboard goes away; taps the field: it comes back;
#     Back puts it away and a tap into the field brings it back again
#
# The key positions are those of the AOSP keyboard (LatinIME) of the
# emulator image, as fractions of the screen.
#
#   bash tools/android/keyboard_check.sh        (ADB, SERIAL from the env)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PKG=org.firn.keyboardtest
OUT=${OUT:-/tmp/firn-keyboard-check}
mkdir -p "$OUT"
fail=0
ok()  { echo "   OK      $1"; }
bad() { echo "   FAILED  $1"; fail=1; }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

echo "== 1. build the APK (x86_64) =="
export FIRNLIB=$ROOT/lib FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
bash tools/android/build.sh examples/fui/form.fi --name "fUi Form" \
    --package $PKG --abi x86_64 2>&1 | tail -1
APK=$ROOT/build/android/fui_form/fui_form.apk
[ -f "$APK" ] || { echo "no APK"; exit 1; }
$ADB root >/dev/null 2>&1; sleep 2; $ADB wait-for-device
$ADB install -r "$APK" | tail -1
$ADB shell settings put system accelerometer_rotation 0
$ADB shell settings put system user_rotation 0
$ADB shell wm user-rotation lock 0 >/dev/null 2>&1
# the soft keyboard is shown although the emulator has a hardware keyboard
$ADB shell settings put secure show_ime_with_hard_keyboard 1
SIZE=$($ADB shell wm size | grep -o '[0-9]*x[0-9]*' | tail -1)
W=${SIZE%x*}; H=${SIZE#*x}
tap() { $ADB shell input tap $(($1 * W / 1000)) $(($2 * H / 1000)); }   # permille of the screen

shown() { $ADB shell dumpsys input_method 2>/dev/null | grep -c 'mInputShown=true'; }
wait_shown() { # 1 = shown, 0 = hidden; at most 30 s
    local i=0
    while [ $i -lt 30 ]; do
        [ "$(shown)" = "$1" ] && return 0
        sleep 1; i=$((i + 1))
    done
    return 1
}
# the pixels of a field: the hashes of a crop (permille box) over some
# seconds, as a sorted set. The caret blinks, so one picture shows it or not
# by chance; the set of what the field looks like contains both states.
crop() { # name x0 y0 x1 y1
    local i=0 all=""
    while [ $i -lt 8 ]; do
        $ADB exec-out screencap -p > "$OUT/$1.png"
        all="$all $(python3 - "$OUT/$1.png" $2 $3 $4 $5 <<'PY'
import sys, hashlib
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGB')
w, h = im.size
x0, y0, x1, y1 = [int(v) for v in sys.argv[2:6]]
c = im.crop((x0 * w // 1000, y0 * h // 1000, x1 * w // 1000, y1 * h // 1000))
print(hashlib.md5(c.tobytes()).hexdigest()[:6])
PY
)"
        sleep 0.35; i=$((i + 1))
    done
    echo $all | tr ' ' '\n' | sort -u | tr '\n' ',' | sed 's/,$//'
}
# keyboard up: where the two fields are (permille), the keys of LatinIME
F1="100 255 900 310"        # first field box, keyboard up
F2="100 315 900 365"        # second field box, keyboard up
# keys: row 1 qwertyuiop at y 692, row 2 asdfghjkl at y 762, backspace at (930, 832)
ROW1=692; ROW2=762
key1() { tap $((50 + 100 * $1)) $ROW1; sleep 0.7; }   # index in qwertyuiop
key2() { tap $((100 + 100 * $1)) $ROW2; sleep 0.7; }  # index in asdfghjkl
KEY_H() { key2 5; }; KEY_E() { key1 2; }; KEY_L() { key2 8; }; KEY_O() { key1 8; }; KEY_A() { key2 0; }
BACKSPACE() { tap 930 832; sleep 0.7; }
FIELD1_DOWN="500 447"; FIELD2_DOWN="500 502"; SEND_DOWN="400 557"; CLEAR_DOWN="595 557"

start() {
    $ADB shell am force-stop $PKG
    $ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
    local i=0
    while [ $i -lt 40 ]; do
        sleep 1; i=$((i + 1))
        $ADB exec-out screencap -p > "$OUT/start.png"
        python3 - "$OUT/start.png" <<'PY' && return 0
import sys
from PIL import Image
import numpy as np
im = np.array(Image.open(sys.argv[1]).convert('RGB'))
sys.exit(0 if (im == [244, 244, 246]).all(axis=2).mean() > 0.6 else 1)
PY
    done
    return 1
}

echo "== 2. the keyboard comes up =="
start; check "the program is on screen" "[ $? = 0 ]"
tap $FIELD1_DOWN
wait_shown 1; check "tap into the first field: the keyboard is shown" "[ $? = 0 ]"
$ADB logcat -d 2>/dev/null | grep -q 'firn *: ime: ready'
check "the input method was built (ime: ready in logcat)" "[ $? = 0 ]"

echo "== 3. typing on the keyboard =="
KEY_H; KEY_E; KEY_L; KEY_L; KEY_O; sleep 2
SOFT_HELLO=$(crop soft_hello $F1); echo "   soft keys 'hello':    $SOFT_HELLO"
BACKSPACE; sleep 2
SOFT_HELL=$(crop soft_hell $F1); echo "   after Backspace:      $SOFT_HELL"
check "Backspace changed the field" "[ '$SOFT_HELLO' != '$SOFT_HELL' ]"
tap 300 330   # the second field (keyboard up)
sleep 2
check "the keyboard stays up when the other field takes the focus" "[ \"\$(shown)\" = 1 ]"
KEY_A; KEY_A; sleep 2
SOFT_AA=$(crop soft_aa $F2); echo "   second field 'aa':    $SOFT_AA"

echo "== 4. the same text arrives some other way: the same pixels =="
# Clear (a button) empties both fields and puts the keyboard away
tap 595 391; wait_shown 0; check "a tap on a button puts the keyboard away" "[ $? = 0 ]"
tap $FIELD1_DOWN; wait_shown 1
$ADB shell input text hello; sleep 2
REF_HELLO=$(crop ref_hello $F1); echo "   input text 'hello':   $REF_HELLO"
check "soft keys 'hello' look exactly like 'hello'" "[ '$SOFT_HELLO' = '$REF_HELLO' ]"
$ADB shell input keyevent KEYCODE_DEL; sleep 2
REF_HELL=$(crop ref_hell $F1); echo "   after DEL:            $REF_HELL"
check "Backspace looks exactly like 'hell'" "[ '$SOFT_HELL' = '$REF_HELL' ]"
tap 300 330; sleep 2
$ADB shell input text aa; sleep 2
REF_AA=$(crop ref_aa $F2); echo "   second field 'aa':    $REF_AA"
check "soft keys in the second field look exactly like 'aa'" "[ '$SOFT_AA' = '$REF_AA' ]"

echo "== 5. away and back =="
tap 400 391; wait_shown 0; check "Send puts the keyboard away" "[ $? = 0 ]"
tap $FIELD1_DOWN; wait_shown 1; check "a tap into the field brings it back" "[ $? = 0 ]"
$ADB shell input keyevent KEYCODE_BACK; wait_shown 0
check "Back puts it away" "[ $? = 0 ]"
tap $FIELD1_DOWN; wait_shown 1
check "a tap into the focused field brings it back again" "[ $? = 0 ]"

[ $fail -eq 0 ] && echo "KEYBOARD PASSED" || echo "KEYBOARD FAILED"
exit $fail
