#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/pointers_check.sh -- REAL FINGERS ON ANDROID (roadmap r111).
#
# examples/fui/touchpad.fi as an APK (fui.app on Android: lib/@android/fui/
# apphost.fi), started with --log. On an emulator with adb root the script
# plays raw multi-touch events into the virtual touch screen (sendevent,
# protocol B: one slot per finger) and reads what the program wrote to
# stdout.txt:
#   tap       one finger down + up        -> down id=0 primary=1, up, tap
#   pan       one finger 300 px sideways  -> pan-start, no tap
#   pinch     two fingers 200 -> 340 px   -> ids 0 and 1, 2nd not primary,
#             scale 100 -> ~170 %
#   lift      the FIRST finger goes up first (the index of the second
#             changes, its id does not) -> up id=0 before up id=1, no tap
#
#   bash tools/android/pointers_check.sh        (ADB, SERIAL from the env)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PKG=org.firn.touchpad
OUT=${OUT:-/tmp/firn-pointers-check}
mkdir -p "$OUT"
fail=0
ok()  { echo "   OK      $1"; }
bad() { echo "   FAILED  $1"; fail=1; }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

echo "== 1. build the APK (x86_64) =="
printf -- '--log\n' > "$OUT/args.txt"
export FIRNLIB=$ROOT/lib FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
bash tools/android/build.sh examples/fui/touchpad.fi --name "fUi Touchpad" \
    --package $PKG --abi x86_64 --args-file "$OUT/args.txt" 2>&1 | tail -1
APK=$ROOT/build/android/fui_touchpad/fui_touchpad.apk
[ -f "$APK" ] || { echo "no APK"; exit 1; }

echo "== 2. install and drive =="
$ADB root >/dev/null 2>&1; sleep 2; $ADB wait-for-device
$ADB install -r "$APK" | tail -1
# the virtual multi-touch screen of the emulator
DEV=$($ADB shell getevent -lp 2>/dev/null | awk '/add device/ {d=$4} /virtio_input_multi_touch_1/ {print d; exit}')
[ -n "$DEV" ] || DEV=/dev/input/event2
SIZE=$($ADB shell wm size | grep -o '[0-9]*x[0-9]*' | tail -1)
XM=${SIZE%x*}; YM=${SIZE#*x}
echo "   touch device $DEV, screen ${XM}x${YM}"

cat > "$OUT/mt.sh" <<EOF
D=$DEV
XM=$XM
YM=$YM
s() { sendevent \$D \$1 \$2 \$3; }
raw() { echo \$(( \$1 * 32767 / \$2 )); }
down() { s 3 47 \$1; s 3 57 \$2; s 3 53 \$(raw \$3 \$XM); s 3 54 \$(raw \$4 \$YM); s 3 58 100; }
move() { s 3 47 \$1; s 3 53 \$(raw \$2 \$XM); s 3 54 \$(raw \$3 \$YM); }
up() { s 3 47 \$1; s 3 57 -1; }
Y=\$((YM * 5 / 8))
CX=\$((XM / 2))
case "\$1" in
tap)
  down 0 100 \$CX \$Y; s 1 330 1; s 0 0 0
  up 0; s 1 330 0; s 0 0 0 ;;
pan)
  down 0 100 \$((CX - 150)) \$Y; s 1 330 1; s 0 0 0
  for k in 1 2 3 4 5 6 7 8 9 10; do move 0 \$((CX - 150 + 30 * k)) \$Y; s 0 0 0; done
  up 0; s 1 330 0; s 0 0 0 ;;
pinch)
  down 0 100 \$((CX - 100)) \$Y; s 1 330 1; s 0 0 0
  down 1 101 \$((CX + 100)) \$Y; s 0 0 0
  for k in 1 2 3 4 5 6 7; do
    move 0 \$((CX - 100 - 10 * k)) \$Y; move 1 \$((CX + 100 + 10 * k)) \$Y; s 0 0 0
  done
  up 1; s 0 0 0
  up 0; s 1 330 0; s 0 0 0 ;;
lift)
  down 0 100 \$((CX - 100)) \$Y; s 1 330 1; s 0 0 0
  down 1 101 \$((CX + 100)) \$Y; s 0 0 0
  up 0; s 0 0 0
  up 1; s 1 330 0; s 0 0 0 ;;
esac
EOF
$ADB push "$OUT/mt.sh" /data/local/tmp/mt.sh >/dev/null

LOG=/sdcard/Android/data/$PKG/files/stdout.txt
run() { # scenario
    $ADB shell am force-stop $PKG
    $ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
    sleep 4
    $ADB shell sh /data/local/tmp/mt.sh "$1" 2>&1 | grep -v '^$' | head -3
    sleep 1.5
    $ADB shell cat $LOG | tr -d '\r' > "$OUT/$1.log"
    $ADB exec-out screencap -p > "$OUT/$1.png"
}

echo "== tap =="
run tap
check "one finger down: id 0, a touch (type 2), primary" "grep -qx 'down id=0 type=2 primary=1' $OUT/tap.log"
check "it goes up and that is one tap" "grep -qx 'tap 1' $OUT/tap.log && grep -q '^up id=0' $OUT/tap.log"

echo "== pan =="
run pan
check "the finger left the slop: a pan starts" "grep -q '^pan-start' $OUT/pan.log"
check "a pan is no tap" "! grep -q '^tap' $OUT/pan.log"

echo "== pinch =="
run pinch
check "two fingers: ids 0 and 1" "grep -qx 'down id=0 type=2 primary=1' $OUT/pinch.log && grep -qx 'down id=1 type=2 primary=0' $OUT/pinch.log"
check "the second finger is not primary" "grep -qx 'down id=1 type=2 primary=0' $OUT/pinch.log"
check "the pinch starts at 100 %" "grep -qx 'pinch-start 100' $OUT/pinch.log"
LAST=$(grep '^pinch ' $OUT/pinch.log | tail -1 | awk '{print $2}')
check "fingers 200 -> 340 px apart: scale ${LAST:-?} % (want 165..175)" "[ -n '$LAST' ] && [ '${LAST:-0}' -ge 165 ] && [ '${LAST:-0}' -le 175 ]"
check "both fingers go up, no tap" "[ \$(grep -c '^up ' $OUT/pinch.log) -eq 2 ] && ! grep -q '^tap' $OUT/pinch.log"

echo "== lift: the first finger goes up first =="
run lift
UP0=$(grep -n '^up id=0' $OUT/lift.log | head -1 | cut -d: -f1)
UP1=$(grep -n '^up id=1' $OUT/lift.log | head -1 | cut -d: -f1)
check "up id=0 comes before up id=1 (the id outlives the index)" "[ -n '$UP0' ] && [ -n '$UP1' ] && [ '${UP0:-9}' -lt '${UP1:-0}' ]"
check "two fingers are no tap" "! grep -q '^tap' $OUT/lift.log"

$ADB shell am force-stop $PKG
[ "$fail" = "0" ] && echo "POINTERS CHECK PASSED" || echo "POINTERS CHECK FAILED"
exit $fail
