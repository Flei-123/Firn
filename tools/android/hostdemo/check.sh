#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/hostdemo/check.sh -- EMULATOR CHECKS OF plat.host (login-free demo + local test server).
#
#   /root/jarvis/bin/heavy bash tools/android/hostdemo/check.sh
#
# Builds tools/android/hostdemo (APK x86_64, --push --pick), boots a throw-away emulator (tools/android/emulator.sh),
# starts server.py on 0.0.0.0:$PORT (the emulator reaches it as 10.0.2.2) and drives the demo with the property
# debug.firn.hostdemo ("<seq> <command> <args>"). The demo logs every answer as "hostdemo ..." (logcat tag firn).
# Exit code 0 = every check passed. Env: PORT, STEPS (default: all), APK (skip the build), USE_RUNNING=1 (an emulator is
# already up on EMU_PORT/ADB_PORT), FIRNC, FIRNLIB.
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
HERE=$ROOT/tools/android/hostdemo
. "$ROOT/tools/android/emulator.sh"
. "$ROOT/tools/android/dismiss_anr.sh"
. "$HERE/lib.sh"
. "$HERE/steps.sh"
PORT=${PORT:-7795}
STEPS=${STEPS:-net state typing notify save share_pick back_route crash}
WORK=$(mktemp -d)
SERVER_LOG=$WORK/server.log
SERVER_PID=""
EMU_OWNED=0
export FIRNLIB=${FIRNLIB:-$ROOT/lib}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNC

cleanup() {
    [ -z "$SERVER_PID" ] || kill "$SERVER_PID" 2>/dev/null
    [ "$EMU_OWNED" = 0 ] || emu_stop
    rm -rf "$WORK" "$ROOT/build/android/hostdemo"
}
trap cleanup EXIT

echo "== build and boot =="
APK=${APK:-$WORK/hostdemo.apk}
if [ ! -f "$APK" ]; then
    bash tools/android/build.sh tools/android/hostdemo/main.fi --name hostdemo --package $PKG \
        --abi x86_64 --push --pick --out "$APK" >"$WORK/build.log" 2>&1 || { tail -20 "$WORK/build.log"; echo "HOSTDEMO FAILED (build)"; exit 1; }
fi
python3 -I "$HERE/server.py" "$PORT" >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
if [ "${USE_RUNNING:-0}" != 1 ]; then emu_start; EMU_OWNED=1; fi
$ADB install -r "$APK" >/dev/null || { echo "HOSTDEMO FAILED (install)"; exit 1; }
dismiss_anr
start_app || { echo "HOSTDEMO FAILED (the demo did not start)"; $ADB logcat -d | tail -20; exit 1; }

for step in $STEPS; do "step_$step"; done
not_checked
echo "HOSTDEMO: $PASS passed, $FAIL failed"
[ $FAIL -eq 0 ]
