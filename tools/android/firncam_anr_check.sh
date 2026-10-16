#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/firncam_anr_check.sh -- GATE: the FirnCam UI thread must never block (ANR "FirnCam is not responding").
#
# Android's input dispatcher gives an app 5 s to read a touch event; FirnCam is a NativeActivity whose main loop (the app
# thread) reads the input queue, so every turn of that loop that takes seconds is an ANR waiting to happen. The app logs
#   firncam: gap ms=N phase=P        one line per main-loop turn longer than 100 ms (P = what ran, P_* in main.fi)
#   firncam: ... max_gap=N gap_phase=P   every 3 s: the longest turn since the last line
# This script installs a FirnCam APK on an emulator, starts it with the zoom sweep FORCED (setprop debug.firncam.sweep 1:
# an emulator camera is no logical multi camera, so the sweep would not run otherwise), taps the screen every 2 s like an
# impatient user, and checks
#   * no am_anr event for the package (the real Android ANR detector)
#   * the longest main-loop turn <= MAX_GAP_MS (default 250; the goal on a phone is 100, an emulator under host load jitters to ~150)
#   * the app is still alive at the end
# Then it presses HOME, comes back, and runs the same two checks again (pause/resume must not block either).
# Part 3 (r655): the shutter, a lens/mode tap during the shot, HOME during a shot, resume. Photo capture, session open/close and
# the burst merge run on the camera worker thread (log "work op=N stat=S ms=M"): the gate needs >= 2 finished operations and
# no CAMERA phase (open 2, capture 5, save 6, zoom 7, scan 1, chips 8) of the UI thread longer than CAM_MAX_MS (default 100).
#
#   SERIAL=emulator-5584 bash tools/android/firncam_anr_check.sh            (builds the APK itself, x86_64)
#   SERIAL=emulator-5584 APK=f.apk bash tools/android/firncam_anr_check.sh   (use a finished x86_64 APK)
# Environment: ADB, SERIAL, APK, FIRNC, MAX_GAP_MS, GL_MAX_MS, RUN_SECONDS (default 24). Exit 0 = pass.
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
ADB=${ADB:-/root/android-sdk/platform-tools/adb}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
PKG=org.firn.firncam
MAX_GAP_MS=${MAX_GAP_MS:-250}
GL_MAX_MS=${GL_MAX_MS:-3500}
RUN_SECONDS=${RUN_SECONDS:-24}
CAM_MAX_MS=${CAM_MAX_MS:-100}
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
FAIL=0
ok()  { echo "   OK      $1"; }
bad() { echo "   FAILED  $1"; FAIL=1; }

APKF=${APK:-}
if [ -z "$APKF" ]; then
    export FIRNLIB=$ROOT/lib FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
    bash "$ROOT/tools/android/build.sh" "$ROOT/examples/firncam/main.fi" --camera --name FirnCam --package $PKG \
        --version-code 1 --version-name gate --abi x86_64 --out "$D/f.apk" > "$D/build.log" 2>&1 \
        || { tail -20 "$D/build.log"; echo "FAIL: build"; exit 1; }
    APKF="$D/f.apk"
fi
$ADB wait-for-device
$ADB install -r "$APKF" > /dev/null 2>&1 || { echo "FAIL: install"; exit 1; }
$ADB shell pm grant $PKG android.permission.CAMERA > /dev/null 2>&1
$ADB shell setprop debug.firncam.sweep 1
# a clean start: no cached sweep result (the cache file of the app, when this build has one)
$ADB shell am force-stop $PKG
$ADB shell "run-as $PKG rm -f files/firncam-learned.txt" > /dev/null 2>&1
$ADB shell "rm -f /sdcard/Android/data/$PKG/files/firncam-learned.txt" > /dev/null 2>&1

phase() { # name: run the app for RUN_SECONDS with taps, then check
    local name=$1
    local i=0
    while [ $i -lt $((RUN_SECONDS / 2)) ]; do
        sleep 2
        $ADB shell input tap 240 400
        i=$((i + 1))
    done
    $ADB logcat -d -s firncam:I > "$D/log_$name.txt"
    $ADB logcat -b events -d > "$D/ev_$name.txt"
    # phase 10 = GL context + shader compile of the preview: needs the thread that owns the GL context; milliseconds on a
    # GPU, a few seconds on the software GL of an emulator -> its own, looser limit (GL_MAX_MS, must stay under the 5 s ANR)
    local worst; worst=$(grep -ao 'gap ms=[0-9]* phase=[0-9]*' "$D/log_$name.txt" | awk '{split($2,a,"=");split($3,b,"="); if (b[2]!=10 && a[2]+0>m) m=a[2]+0} END{print m+0}')
    local glw; glw=$(grep -ao 'gap ms=[0-9]* phase=10' "$D/log_$name.txt" | awk '{split($2,a,"="); if (a[2]+0>m) m=a[2]+0} END{print m+0}')
    local mg; mg=$(grep -ao 'max_gap=[0-9]* gap_phase=[0-9]*' "$D/log_$name.txt" | awk '{split($1,a,"=");split($2,b,"="); if (b[2]!=10 && a[2]+0>m) m=a[2]+0} END{print m+0}')
    [ "$mg" -gt "$worst" ] && worst=$mg
    local lines; lines=$(grep -ac 'ui_fps=' "$D/log_$name.txt")
    echo "   [$name] stats lines=$lines  longest gap=${worst} ms (limit $MAX_GAP_MS), GL init gap=${glw} ms (limit $GL_MAX_MS)"
    grep -a 'gap ms=' "$D/log_$name.txt" | head -5 | cut -c1-120
    [ "$lines" -ge 2 ] && ok "$name: app runs and logs" || bad "$name: no stats lines (app dead or never ready)"
    [ "$glw" -le "$GL_MAX_MS" ] && ok "$name: GL init gap <= $GL_MAX_MS ms" || bad "$name: GL init blocked ${glw} ms"
    [ "$worst" -le "$MAX_GAP_MS" ] && ok "$name: UI thread never blocked > $MAX_GAP_MS ms" || bad "$name: UI thread blocked ${worst} ms"
    if grep -a 'am_anr' "$D/ev_$name.txt" | grep -aq "$PKG"; then bad "$name: Android reported an ANR"; else ok "$name: no ANR event"; fi
}

echo "== 1. cold start with forced zoom sweep, tapping =="
$ADB logcat -c; $ADB logcat -b events -c
$ADB shell am start -n $PKG/android.app.NativeActivity > /dev/null
phase start

echo "== 2. HOME, back, tapping =="
$ADB shell input keyevent KEYCODE_HOME
sleep 3
$ADB logcat -c; $ADB logcat -b events -c
$ADB shell am start -n $PKG/android.app.NativeActivity > /dev/null
phase resume

echo "== 3. shutter, tap during the shot, HOME during the shot, resume =="
SZ=$($ADB shell wm size | grep -o '[0-9]*x[0-9]*' | tail -1); SW=${SZ%x*}; SH=${SZ#*x}
tapf() { $ADB shell input tap $(( SW * $1 / 1080 )) $(( SH * $2 / 1920 )); }   # coordinates of a 1080x1920 screen
$ADB logcat -c; $ADB logcat -b events -c
$ADB shell am force-stop $PKG
$ADB shell am start -n $PKG/android.app.NativeActivity > /dev/null
sleep 14
tapf 540 1705            # shutter
sleep 1; tapf 659 1588   # mode tab while the shot runs (ignored or deferred, never a block)
tapf 425 1503            # zoom chip
sleep 12                 # the first shot finishes
tapf 540 1705            # second shot ...
sleep 2
$ADB shell input keyevent KEYCODE_HOME   # ... paused in the middle of it
sleep 5
$ADB shell am start -n $PKG/android.app.NativeActivity > /dev/null
sleep 3; tapf 540 1705; sleep 12
$ADB logcat -d -s firncam:I > "$D/log_shot.txt"
$ADB logcat -b events -d > "$D/ev_shot.txt"
camw=$(grep -ao 'gap ms=[0-9]* phase=[0-9]* phase_ms=[0-9]*' "$D/log_shot.txt" | awk '{split($2,a,"=");split($3,b,"="); if ((b[2]==1||b[2]==2||b[2]==5||b[2]==6||b[2]==7||b[2]==8) && a[2]+0>m) m=a[2]+0} END{print m+0}')
allw=$(grep -ao 'gap ms=[0-9]* phase=[0-9]*' "$D/log_shot.txt" | awk '{split($2,a,"=");split($3,b,"="); if (b[2]!=10 && a[2]+0>m) m=a[2]+0} END{print m+0}')
nwork=$(grep -ac 'work op=' "$D/log_shot.txt")
echo "   [shot] worker operations=$nwork  longest camera-phase gap=${camw} ms (limit $CAM_MAX_MS), longest gap=${allw} ms (limit $MAX_GAP_MS)"
grep -a 'work op=' "$D/log_shot.txt" | cut -c20-90
[ "$nwork" -ge 2 ] && ok "shot: worker ran >= 2 operations" || bad "shot: worker ran $nwork operations"
[ "$camw" -le "$CAM_MAX_MS" ] && ok "shot: UI thread camera phases <= $CAM_MAX_MS ms" || bad "shot: UI thread blocked ${camw} ms in a camera phase"
[ "$allw" -le "$MAX_GAP_MS" ] && ok "shot: UI thread never blocked > $MAX_GAP_MS ms" || bad "shot: UI thread blocked ${allw} ms"
if grep -a 'am_anr' "$D/ev_shot.txt" | grep -aq "$PKG"; then bad "shot: Android reported an ANR"; else ok "shot: no ANR event"; fi
$ADB shell pidof $PKG > /dev/null && ok "shot: app alive after the pause" || bad "shot: app died (crash)"

$ADB shell setprop debug.firncam.sweep 0
$ADB shell am force-stop $PKG
[ $FAIL = 0 ] && echo "PASS: firncam_anr_check" || echo "FAIL: firncam_anr_check"
exit $FAIL
