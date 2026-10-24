#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/ime_inject_check.sh -- composition, umlauts, emoji, deletion and Enter
# on the on-screen keyboard path of a fUi text field (roadmap r70), on an emulator.
#
# tools/android/imetest_app.fi is an APK whose pump reads the property debug.firn.ime
# ("<seq> <command> <args>") and hands the edit to lib/android/inputmethod.fi exactly
# like the keyboard's InputConnection does (lib/android/imeinject.fi). The text of
# the first field is logged as hex octets ("imetext=").
#
#   bash tools/android/ime_inject_check.sh        (ADB, SERIAL, FIRNC from the env)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PKG=org.firn.imetest
OUT=${OUT:-$(mktemp -d)}
mkdir -p "$OUT"
fail=0
. "$ROOT/tools/android/dismiss_anr.sh"
ok()  { echo "   OK      $1"; }
bad() { echo "   FAILED  $1"; fail=1; $ADB exec-out screencap -p > "$OUT/fail_$RANDOM.png" 2>/dev/null; }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

echo "== 1. build the APK (x86_64) =="
export FIRNLIB=$ROOT/lib FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
bash tools/android/build.sh tools/android/imetest_app.fi --name "IME test" \
    --package $PKG --abi x86_64 2>&1 | tail -1
APK=$ROOT/build/android/ime_test/ime_test.apk
[ -f "$APK" ] || { echo "no APK"; exit 1; }
$ADB root >/dev/null 2>&1; sleep 2; $ADB wait-for-device
$ADB install -r "$APK" | tail -1
set_rotation 0 || echo "   FAILED  portrait rotation did not apply"
dismiss_anr
$ADB shell settings put secure show_ime_with_hard_keyboard 1
SIZE=$($ADB shell wm size | grep -o '[0-9]*x[0-9]*' | tail -1)
W=${SIZE%x*}; H=${SIZE#*x}
tap() { $ADB shell input tap $(($1 * W / 1000)) $(($2 * H / 1000)); }
shown() { $ADB shell dumpsys input_method 2>/dev/null | grep -c 'mInputShown=true'; }
wait_shown() {
    local i=0
    while [ $i -lt 30 ]; do
        [ "$(shown)" = "$1" ] && return 0
        sleep 1; i=$((i + 1))
    done
    return 1
}
crop() { # name [x0 y0 x1 y1] in 1/1000 of the screen; default: the name field
    local i=0 all="" box="${2:-100} ${3:-195} ${4:-900} ${5:-240}"
    while [ $i -lt 6 ]; do
        $ADB exec-out screencap -p > "$OUT/$1.png"
        all="$all $(python3 - "$OUT/$1.png" $box <<'PY'
import sys, hashlib
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGB')
w, h = im.size
x0, y0, x1, y1 = [int(v) for v in sys.argv[2:6]]
c = im.crop((x0 * w // 1000, y0 * h // 1000, x1 * w // 1000, y1 * h // 1000))
print(hashlib.md5(c.tobytes()).hexdigest()[:8])
PY
)"
        sleep 0.3; i=$((i + 1))
    done
    echo $all | tr ' ' '\n' | sort -u | tr '\n' ','
}
disjoint() { # two comma lists share no hash
    python3 -c 'import sys; a=set(sys.argv[1].split(","))-{""}; b=set(sys.argv[2].split(","))-{""}; sys.exit(0 if not (a & b) else 1)' "$1" "$2"
}
SEQ=0
inject() { # command and arguments; waits until the app has logged the answer
    SEQ=$((SEQ + 1))
    $ADB shell setprop debug.firn.ime "\"$SEQ $*\""
    local i=0
    while [ $i -lt 40 ]; do
        $ADB logcat -d -s firn:I 2>/dev/null | grep -q "imeinject seq=$SEQ ok=1" && break
        sleep 0.25; i=$((i + 1))
    done
    sleep 1.5
}
text() { $ADB logcat -d -s firn:I 2>/dev/null | grep -o 'imetext=[0-9a-f]*' | tail -1 | sed 's/imetext=//'; }
expect() { check "$1" "[ \"\$(text)\" = '$2' ]"; }

echo "== 2. the field takes the focus =="
$ADB logcat -c
$ADB shell setprop debug.firn.ime '""'
$ADB shell am force-stop $PKG
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
sleep 8
dismiss_anr
tap 500 385
wait_shown 1; check "tap into the field: the keyboard is shown" "[ $? = 0 ]"
sleep 2
$ADB logcat -d 2>/dev/null | grep -q 'firn *: ime: ready'
check "the input method was built" "[ $? = 0 ]"

echo "== 3. commit, umlaut, composition =="
inject commit 47 72
expect "commit 'Gr'" 4772
inject comp fc
expect "composing text u-umlaut lands in the field" 4772c3bc
COMP=$(crop comp)
inject finish
sleep 1
FIN=$(crop fin)
check "finishing the composition removes the underline (pixels change)" "disjoint '$COMP' '$FIN'"
expect "the text stays after finishing" 4772c3bc
inject comp 64 e4
expect "cumulative composing: 'da-umlaut'" 4772c3bc64c3a4
inject comp 64 e4 df
expect "composing replaces the composing word" 4772c3bc64c3a4c39f
inject finish

echo "== 4. emoji and deletion =="
inject commit 1f600
expect "emoji (surrogate pair) arrives as one character" 4772c3bc64c3a4c39ff09f9880
inject delsur 2 0
expect "deleteSurroundingText(2,0) removes the whole emoji" 4772c3bc64c3a4c39f
inject commit 1f600
inject delcp 1 0
expect "deleteSurroundingTextInCodePoints(1,0) removes the emoji" 4772c3bc64c3a4c39f
inject key 67
expect "Backspace key removes the sharp s" 4772c3bc64c3a4
inject delsur 1 0
expect "deleteSurroundingText(1,0) removes the a-umlaut" 4772c3bc64

echo "== 5. Enter =="
inject enter
sleep 1
check "the Enter action reached the program" "$ADB logcat -d -s firn:I 2>/dev/null | grep -q 'imenote=enter'"

echo "== 5b. multi-line text area =="
tap 500 365
sleep 2
inject commit 48 69 a 4e
expect "text area: commit across a line break" 48690a4e
inject comp fc
expect "text area: composing text lands on the second line" 48690a4ec3bc
AC=$(crop acomp 100 308 900 420)
inject finish
sleep 1
AF=$(crop afin 100 308 900 420)
check "text area: the underline of the composing word is drawn and goes away" "disjoint '$AC' '$AF'"
inject comp 64 e4
expect "text area: composing replaces the composing word" 48690a4ec3bc64c3a4
inject finish
inject delsur 1 0
expect "text area: deleteSurroundingText removes the a-umlaut" 48690a4ec3bc64
inject key 67
inject key 67
inject key 67
inject key 67
expect "text area: Backspace removes d, u-umlaut, N and the line break" 4869

echo "== 6. no timeouts, no crash =="
check "no 'ime: request timed out' in the log" "! $ADB logcat -d 2>/dev/null | grep -q 'ime: .*timed out'"
check "the process is alive" "$ADB shell pidof $PKG | grep -q ."

[ $fail -eq 0 ] && echo "IME INJECT PASSED" || echo "IME INJECT FAILED"
exit $fail
