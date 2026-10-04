#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/appkit/android_check.sh -- THE UPDATE ON ANDROID, end to end, on the
# x86-64 emulator (or a phone over adb), against a LOCAL store.
#
# What runs: the real store tool (orientstore `store init`, `add`) makes a
# signed catalog in a temp directory; tools/appkit/teststore_server.py serves
# it on this machine's loopback (the emulator sees it as 10.0.2.2);
# tools/appkit/android_check/main.fi -- a program built on lib/appkit -- is
# built four times by tools/android/build.sh (0.1.0 installed like a sideload,
# 0.2.0 and 0.2.1 in the store, 0.3.0 signed with ANOTHER key), together with
# the receiver class
# org.firn.FirnInstall that tools/appkit/installdex_main.fi writes. Then:
#
#   platform and package kind, locale (JNI)
#   check: "update available"; the download on the app thread (no threads
#   on Android); a changed byte is refused; a wrong signature is refused;
#   the hand-over to the PackageInstaller (APPLY 6 = staged); the user
#   says no (the receiver reports status 3); the user says yes (the receiver
#   starts the system's confirmation, the app is replaced by 0.2.0); the next
#   update needs no question (the app is the installer of record now);
#   "up to date" afterwards; an APK signed with another key is refused by
#   Android and the receiver reports it (install-status.txt), the app keeps
#   running.
#
# Nothing here touches the live store: the store is http://10.0.2.2:<port>/.
#
#   bash tools/appkit/android_check.sh        (heavy: via /root/jarvis/bin/heavy)
#   ADB=... EMULATOR=... AVD=firn SERIAL=... FIRNC=... ORIENTSTORE_TOOL=...
#
# A device that is already up is used and left running; otherwise the AVD
# is started (no window) and stopped again at the end.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB=$ROOT/lib
SDK=${SDK:-/root/android-sdk}
ADB=${ADB:-$SDK/platform-tools/adb}
EMULATOR=${EMULATOR:-$SDK/emulator/emulator}
AVD=${AVD:-firn}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
STORE_TOOL=${ORIENTSTORE_TOOL:-}
if [ -z "$STORE_TOOL" ]; then
    for c in /root/orientstore-wt-apps/werkzeug/store /root/orientstore/werkzeug/store; do
        if [ -f "$c" ] && grep -q 'add-app' "$c"; then STORE_TOOL=$c; break; fi
    done
fi
skip() { echo "SKIP: $1"; exit 0; }
[ -x "$FIRNC" ] || skip "no firnc"
[ -n "$STORE_TOOL" ] || skip "needs an orientstore tool with 'add-app' (set ORIENTSTORE_TOOL)"
python3 -c 'import cryptography' 2>/dev/null || skip "python3 cryptography is missing"
[ -x "${ADB%% *}" ] || skip "no adb"
[ -d "${NDK:-/root/android-ndk-min}" ] || skip "no Android NDK (see tools/android/build.sh)"
command -v keytool >/dev/null 2>&1 || skip "no keytool (a JDK) for the signing keys"

PKG=org.firn.appkitcheck
FILES=/sdcard/Android/data/$PKG/files
W=$(mktemp -d "${TMPDIR:-/tmp}/appkit-android.XXXXXX")
SRV_PID=""
EMU_PID=""
cleanup() {
    [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null
    $ADB shell am force-stop $PKG >/dev/null 2>&1
    $ADB uninstall $PKG >/dev/null 2>&1
    if [ -n "$EMU_PID" ]; then
        $ADB emu kill >/dev/null 2>&1
        sleep 2
        kill "$EMU_PID" 2>/dev/null
    fi
    rm -rf "$W" "$ROOT/build/android/appkit_check"
}
trap cleanup EXIT

PASS=0
FAIL=0
FAILED=""
ok()  { PASS=$((PASS + 1)); printf '  ok    %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); FAILED="$FAILED\n  - $1"; printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2" | head -12; }
expect() { if printf '%s' "$2" | grep -qF -- "$3"; then ok "$1"; else bad "$1" "wanted '$3' in: $2"; fi; }
refute() { if printf '%s' "$2" | grep -qF -- "$3"; then bad "$1" "did NOT want '$3' in: $2"; else ok "$1"; fi; }

echo "== 1. the device"
booted() { [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; }
if ! booted; then
    export ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=${ANDROID_AVD_HOME:-$HOME/.android/avd}
    [ -x "$EMULATOR" ] || skip "no running device and no emulator"
    "$EMULATOR" -avd "$AVD" -no-window -no-audio -no-boot-anim -no-snapshot \
        -gpu swiftshader_indirect >"$W/emu.log" 2>&1 &
    EMU_PID=$!
    for i in $(seq 1 120); do
        sleep 3
        booted && break
    done
    booted || { echo "the emulator did not boot"; tail -5 "$W/emu.log"; exit 1; }
    sleep 5
fi
echo "   $($ADB shell getprop ro.product.cpu.abi | tr -d '\r') API $($ADB shell getprop ro.build.version.sdk | tr -d '\r')"
ABI=$($ADB shell getprop ro.product.cpu.abi | tr -d '\r')
case "$ABI" in
    arm64-v8a) BUILD_ABI=arm64; PLAT=android-arm64 ;;
    x86_64) BUILD_ABI=x86_64; PLAT=android-x86_64 ;;
    *) skip "the device ABI $ABI is not one of arm64-v8a, x86_64" ;;
esac

echo "== 2. the store"
export ORIENTSTORE_REPO=$W/repo
STORE="python3 $STORE_TOOL"
$STORE init --name "Android check store" --adresse "http://10.0.2.2/" >"$W/init.log" 2>&1 || { cat "$W/init.log"; exit 1; }
KEY=$(python3 "$STORE_TOOL" key)
python3 tools/appkit/teststore_server.py --root "$ORIENTSTORE_REPO" --log "$W/srv.log" >"$W/srv.out" 2>&1 &
SRV_PID=$!
for i in $(seq 1 50); do grep -q '^PORT ' "$W/srv.out" 2>/dev/null && break; sleep 0.1; done
PORT=$(sed -n 's/^PORT //p' "$W/srv.out" | head -1)
[ -n "$PORT" ] || { echo "no server"; cat "$W/srv.out"; exit 1; }
echo "   key $KEY, port $PORT"

echo "== 3. the receiver class and the programs"
"$FIRNC" -o "$W/installdex" tools/appkit/installdex_main.fi 2>/dev/null || { echo "installdex did not build"; exit 1; }
"$W/installdex" "$W/classes.dex" firnapp || { echo "installdex failed"; exit 1; }
printf '<receiver android:name="org.firn.FirnInstall" android:exported="false"/>\n' >"$W/receiver.xml"
mkdir -p "$W/apk"
build() {   # build <version> <versionCode> <keystore> <out>
    FIRN_E2E_VERSION=$1 FIRN_E2E_STORE=http://10.0.2.2:$PORT/ FIRN_E2E_KEY=$KEY \
    FIRNC=$FIRNC KEYSTORE=$3 \
        bash tools/android/build.sh tools/appkit/android_check/main.fi --name "Appkit Check" \
        --package $PKG --version-code "$2" --version-name "$1" --abi $BUILD_ABI \
        --permission android.permission.INTERNET \
        --permission android.permission.REQUEST_INSTALL_PACKAGES \
        --permission android.permission.UPDATE_PACKAGES_WITHOUT_USER_ACTION \
        --dex "$W/classes.dex" --manifest-extra "$W/receiver.xml" --out "$4" >"$W/build.log" 2>&1 \
        || { echo "build of $1 failed:"; tail -20 "$W/build.log"; exit 1; }
}
build 0.1.0 100 "$W/key1.jks" "$W/apk/check-0.1.0.apk"
build 0.2.0 200 "$W/key1.jks" "$W/apk/check-0.2.0.apk"
build 0.2.1 210 "$W/key1.jks" "$W/apk/check-0.2.1.apk"
build 0.3.0 300 "$W/key2.jks" "$W/apk/check-0.3.0.apk"
ls -la "$W/apk" | awk 'NR>1{print "   " $5 " " $9}'
AAPT2=$(ls "$SDK"/build-tools/*/aapt2 2>/dev/null | tail -1)
if [ -n "$AAPT2" ]; then
    perms=$("$AAPT2" dump badging "$W/apk/check-0.2.0.apk" 2>/dev/null | grep -c "uses-permission")
    expect "the manifest asks for internet and the installer permissions" "$perms" 3
fi
$STORE add --aenderungen "release 0.2.0" "$W/apk/check-0.2.0.apk" >"$W/publish-0.2.0.log" 2>&1 \
    || { echo "publish failed"; cat "$W/publish-0.2.0.log"; exit 1; }
PAYLOAD=$ORIENTSTORE_REPO/$(python3 - <<PY
import json
d = json.load(open("$ORIENTSTORE_REPO/index.json"))
print(d["pakete"]["$PKG"]["builds"][0]["datei"])
PY
)
cp "$PAYLOAD" "$W/payload.good"

# ---- helpers on the device
run_app() {   # run_app <command>: writes args.txt, starts the program fresh
    $ADB shell am force-stop $PKG
    printf '%s\n' "$1" >"$W/args.txt"
    $ADB shell "mkdir -p $FILES" >/dev/null 2>&1
    $ADB push "$W/args.txt" "$FILES/args.txt" >/dev/null 2>&1
    $ADB shell "rm -f $FILES/stdout.txt $FILES/install-status.txt" >/dev/null 2>&1
    $ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null 2>&1
}
app_out() { $ADB shell "cat $FILES/stdout.txt 2>/dev/null" | tr -d '\r'; }
wait_for() {  # wait_for <regex> [seconds]: until the program's output has it
    local n=${2:-60} i
    for i in $(seq 1 "$n"); do
        if app_out | grep -qE -- "$1"; then return 0; fi
        sleep 1
    done
    return 1
}
installed_version() {
    $ADB shell dumpsys package $PKG 2>/dev/null | tr -d '\r' | sed -n 's/^ *versionName=//p' | head -1
}
# press a button of the system's dialog by its text (the screen is the only
# way in: the dialog belongs to another app)
tap_button() {  # tap_button <regex>
    $ADB shell uiautomator dump /sdcard/appkit-ui.xml >/dev/null 2>&1
    $ADB shell cat /sdcard/appkit-ui.xml 2>/dev/null | python3 -c '
import re, sys
rx = re.compile(sys.argv[1], re.I)
x = sys.stdin.read()
for m in re.finditer(r"<node[^>]*?text=\"([^\"]*)\"[^>]*?clickable=\"true\"[^>]*?bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", x):
    if rx.search(m.group(1)):
        a, b, c, d = map(int, m.groups()[1:])
        print((a + c) // 2, (b + d) // 2)
        sys.exit(0)
sys.exit(1)' "$1"
}
press() {  # press <regex>: tap that button if it is there
    local xy
    xy=$(tap_button "$1") || return 1
    $ADB shell input tap $xy
    return 0
}
# did the receiver see "the system asks the user" since the last logcat -c?
asked_count() { $ADB logcat -d -s appkit:I 2>/dev/null | grep -c 'the system asks the user'; }
screen_text() {
    $ADB shell uiautomator dump /sdcard/appkit-ui.xml >/dev/null 2>&1
    $ADB shell cat /sdcard/appkit-ui.xml 2>/dev/null | python3 -c '
import re, sys
print(" | ".join(m for m in re.findall(r"text=\"([^\"]+)\"", sys.stdin.read())))'
}

echo "== 4. the program, installed as 0.1.0 (a sideload: adb is the installer)"
$ADB uninstall $PKG >/dev/null 2>&1
$ADB install -r "$W/apk/check-0.1.0.apk" >"$W/install.log" 2>&1 || { cat "$W/install.log"; exit 1; }
$ADB shell pm grant $PKG android.permission.INTERNET >/dev/null 2>&1
run_app ""
wait_for '^HEALTHY' 40 || bad "the program did not start" "$(app_out)"
out=$(app_out)
expect "it runs as 0.1.0" "$out" "RUNNING 0.1.0"
expect "it knows its platform" "$out" "PLATFORM $PLAT"
expect "its package kind is apk" "$out" "ART apk"
expect "the locale came through JNI (a two letter code)" "$(printf '%s\n' "$out" | grep '^LOCALE')" "LOCALE "
dirs=$($ADB shell "ls $FILES" | tr -d '\r' | tr '\n' ' ')
expect "appkit made its directories in the app's files" "$dirs" "state"

echo "== 5. check, download, refusals (all on the app thread: no threads on Android)"
run_app check
wait_for '^RESULT' 60
out=$(app_out)
expect "check: update available" "$out" "RESULT update available err=0 ver=0.2.0"
expect "check: size is the file's" "$out" "size=$(stat -c %s "$W/apk/check-0.2.0.apk")"

python3 - "$PAYLOAD" <<'PY'
import sys
p = sys.argv[1]
b = bytearray(open(p, "rb").read())
b[len(b) // 2] ^= 1
open(p, "wb").write(bytes(b))
PY
run_app update
wait_for '^RESULT (error|ready)' 90
out=$(app_out)
expect "a changed byte in the download is refused (hash)" "$out" "err=7"
refute "and nothing was handed to the system" "$out" "APPLY"
cp "$W/payload.good" "$PAYLOAD"

cp "$ORIENTSTORE_REPO/entry.json.sig" "$W/entry.sig.good"
python3 - "$ORIENTSTORE_REPO/entry.json.sig" <<'PY'
import sys
p = sys.argv[1]
b = bytearray(open(p, "rb").read())
b[10] ^= 1
open(p, "wb").write(bytes(b))
PY
run_app check
wait_for '^RESULT' 60
expect "a wrong signature is refused" "$(app_out)" "err=2"
cp "$W/entry.sig.good" "$ORIENTSTORE_REPO/entry.json.sig"

echo "== 6. the system installer asks, the user says no"
# Android shows its own question; without the user's switch for this app it
# is "not allowed to install unknown apps". The answer "cancel" must come back
# as a status, and the program must go on running.
$ADB shell appops set $PKG REQUEST_INSTALL_PACKAGES deny >/dev/null 2>&1
run_app update
wait_for '^APPLY' 90
expect "the APK was handed to the PackageInstaller (staged)" "$(app_out)" "APPLY 6"
for i in $(seq 1 30); do
    sleep 1
    txt=$(screen_text)
    case "$txt" in *CANCEL*|*Cancel*) break ;; esac
done
if press '^cancel$'; then ok "the system asked and the dialog was cancelled"; else bad "no system dialog to cancel" "$(screen_text)"; fi
for i in $(seq 1 20); do
    $ADB shell "test -f $FILES/install-status.txt" 2>/dev/null && break
    sleep 1
done
st=$($ADB shell "cat $FILES/install-status.txt 2>/dev/null" | tr -d '\r')
expect "the receiver got the answer: aborted (status 3)" "$st" "status=3"
expect "and Android's message" "$st" "INSTALL_FAILED_ABORTED"
expect "the program is still 0.1.0" "$(installed_version)" "0.1.0"
if [ -n "$($ADB shell pidof $PKG | tr -d '\r')" ]; then ok "and still running"; else bad "the program is no longer running"; fi

echo "== 7. the update"
$ADB shell appops set $PKG REQUEST_INSTALL_PACKAGES allow >/dev/null 2>&1
$ADB logcat -c
run_app update
wait_for '^APPLY' 90
expect "update: ready, then staged for the system" "$(app_out)" "RESULT ready to install err=0 ver=0.2.0"
expect "update: the system installer has it (APPLY 6)" "$(app_out)" "APPLY 6"
asked=0
for i in $(seq 1 90); do
    v=$(installed_version)
    [ "$v" = 0.2.0 ] && break
    if press '^(update|install)$'; then asked=1; fi
    sleep 1
done
# Android decides whether to ask: an app that updates ITSELF with
# UPDATE_PACKAGES_WITHOUT_USER_ACTION is updated silently, but the system
# throttles silent updates per app and then asks. Both ways must work; the
# receiver's question is pressed when it comes.
echo "   system questions seen by the receiver: $(asked_count); buttons pressed by this script: $asked"
expect "Android replaced the program: 0.2.0" "$(installed_version)" "0.2.0"
expect "the installer of record is the program itself now" \
    "$($ADB shell dumpsys package $PKG | tr -d '\r' | grep -m1 'installerPackageName')" "installerPackageName=$PKG"

echo "== 8. the new version"
run_app check
wait_for '^RESULT' 60
out=$(app_out)
expect "it runs as 0.2.0" "$out" "RUNNING 0.2.0"
expect "and finds nothing newer" "$out" "RESULT up to date"

echo "== 8b. the next update"
$STORE add --aenderungen "release 0.2.1" "$W/apk/check-0.2.1.apk" >"$W/publish-0.2.1.log" 2>&1 \
    || { echo "publish 0.2.1 failed"; cat "$W/publish-0.2.1.log"; exit 1; }
$ADB logcat -c
run_app update
wait_for '^APPLY' 90
expect "0.2.1 is offered and staged" "$(app_out)" "APPLY 6"
asked=0
for i in $(seq 1 90); do
    v=$(installed_version)
    [ "$v" = 0.2.1 ] && break
    if press '^(update|install)$'; then asked=1; fi
    sleep 1
done
expect "Android replaced the program: 0.2.1" "$(installed_version)" "0.2.1"
echo "   system questions seen by the receiver: $(asked_count); buttons pressed by this script: $asked"
if [ "$(asked_count)" -ge 1 ]; then
    if [ "$asked" = 1 ]; then ok "the system asked (silent updates are throttled); the receiver opened the question and it was answered"
    else bad "the system asked but the receiver's question never reached the screen" "$(screen_text)"; fi
fi

echo "== 9. an APK signed with another key"
$STORE add --schluesselwechsel --aenderungen "release 0.3.0 (other key)" "$W/apk/check-0.3.0.apk" >"$W/publish-0.3.0.log" 2>&1 \
    || { echo "publish 0.3.0 failed"; cat "$W/publish-0.3.0.log"; exit 1; }
run_app update
wait_for '^APPLY' 90
expect "the other-key update downloads fine (the store signed it)" "$(app_out)" "RESULT ready to install err=0 ver=0.3.0"
for i in $(seq 1 60); do
    $ADB shell "test -f $FILES/install-status.txt" 2>/dev/null && break
    press '^(update|install)$' >/dev/null 2>&1
    sleep 1
done
st=$($ADB shell "cat $FILES/install-status.txt 2>/dev/null" | tr -d '\r')
refute "Android refused it: a failure status, not a success" "$st" "status=0"
expect "the receiver wrote what Android said" "$st" "status="
expect "the program is still 0.2.1" "$(installed_version)" "0.2.1"
echo "   Android said: $(printf '%s' "$st" | tr '\n' ' ')"

echo
echo "android check: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || { printf 'failed:%b\n' "$FAILED"; exit 1; }
exit 0
