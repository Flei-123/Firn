#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/dns_check.sh -- DNS on Android (roadmap r86 + r74), on an emulator.
#
#   A. tools/android/dns_check/main.fi as an app: lib/@android/net/dnsconf.fi runs
#      (bionic property reader linked, no crash), the resolver of dns.fi asks a
#      fake DNS server on the host (tools/dns/fake_dns.py, reached as 10.0.2.2)
#      and answers by name; the default resolver reaches the real network.
#   B. demos/pushdemo as the push client with a relay NAME: the push thread
#      resolves `relay.test` (lib/net/dnsmini.fi, asking the fake server given
#      by push_set_dns) and connects to the test relay on the host.
#   C. TLS with a key pin (push_set_pin, lib/tls/tlsmini.fi) against
#      `openssl s_server` (P-256, TLS 1.3, ChaCha20): the right pin shows the
#      line as a notification, a wrong pin shows nothing ("tls handshake failed").
#   D. IPv6: `dual6.test` has a dead A record and AAAA fec0::2 (the host as the
#      emulator sees it over IPv6): plain, and TLS over IPv6.
#
#   bash tools/android/dns_check.sh      (ADB, SERIAL, EMULATOR, AVD, FIRNC from env)
#
# An emulator that is already up is used and left running; otherwise AVD
# (default firn) is booted headless and killed afterwards.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
SDK=${SDK:-/root/android-sdk}
ADB=${ADB:-$SDK/platform-tools/adb}
EMULATOR=${EMULATOR:-$SDK/emulator/emulator}
AVD=${AVD:-firn}
[ -n "${SERIAL:-}" ] && ADB="$ADB -s $SERIAL"
export FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
W=$(mktemp -d "${TMPDIR:-/tmp}/dnschk.XXXXXX")
PASS=0; FAIL=0; EMU_PID=""; DNS_PID=""; RELAY_PID=""; TLS_PID=""; V6_PID=""
cleanup() {
    [ -n "$DNS_PID" ] && kill "$DNS_PID" 2>/dev/null
    [ -n "$RELAY_PID" ] && kill "$RELAY_PID" 2>/dev/null
    [ -n "$V6_PID" ] && kill "$V6_PID" 2>/dev/null
    [ -n "$TLS_PID" ] && kill "$TLS_PID" 2>/dev/null
    if [ -n "$EMU_PID" ]; then $ADB emu kill >/dev/null 2>&1; sleep 2; kill "$EMU_PID" 2>/dev/null; fi
    rm -rf "$W" "$ROOT/build/android/dnscheck" "$ROOT/build/android/firn_push"
}
trap cleanup EXIT
ok()  { PASS=$((PASS+1)); echo "  ok    $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL  $1"; [ -n "${2:-}" ] && echo "        $2" | head -6; }
has() { if printf '%s\n' "$2" | grep -qE -- "$3"; then ok "$1"; else bad "$1" "wanted /$3/ in: $2"; fi; }

echo "== 1. the device"
booted() { [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; }
if ! booted; then
    export ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=${ANDROID_AVD_HOME:-$HOME/.android/avd}
    [ -x "$EMULATOR" ] || { echo "SKIP: no device and no emulator"; exit 0; }
    "$EMULATOR" -avd "$AVD" -no-window -no-audio -no-boot-anim -no-snapshot \
        -gpu swiftshader_indirect >"$W/emu.log" 2>&1 &
    EMU_PID=$!
    for i in $(seq 1 120); do sleep 3; booted && break; done
    booted || { echo "the emulator did not boot"; tail -5 "$W/emu.log"; exit 1; }
    sleep 5
fi
ABI=$($ADB shell getprop ro.product.cpu.abi | tr -d '\r')
case "$ABI" in arm64-v8a) BA=arm64 ;; x86_64) BA=x86_64 ;; *) echo "SKIP: ABI $ABI"; exit 0 ;; esac
echo "   $ABI API $($ADB shell getprop ro.build.version.sdk | tr -d '\r')"

echo "== 2. the fake DNS server and the relay on the host"
python3 tools/dns/fake_dns.py >"$W/dns.out" 2>"$W/dns.log" &
DNS_PID=$!
for i in $(seq 1 50); do grep -q '^port ' "$W/dns.out" 2>/dev/null && break; sleep 0.1; done
DPORT=$(sed -n 's/^port //p' "$W/dns.out" | head -1)
RPORT=${RELAY_PORT:-7793}
python3 tools/android/relay_test.py "$RPORT" --v6 --spacing 1 "dns-check-line" >"$W/relay.log" 2>&1 &
RELAY_PID=$!
echo "   dns 127.0.0.1:$DPORT  relay :$RPORT"

echo "== 3. A: the resolver inside an app"
printf '10.0.2.2\n%s\nplain.test\nnx.test\nchain.test\nsilent.test\n~example.com\n' "$DPORT" >"$W/args.txt"
bash tools/android/build.sh tools/android/dns_check/main.fi --name dnscheck --package org.firn.dnscheck \
    --abi "$BA" --permission android.permission.INTERNET --args-file "$W/args.txt" \
    --out "$W/dnscheck.apk" >"$W/build_a.log" 2>&1 || { cat "$W/build_a.log" | grep -v RWX | tail; exit 1; }
PKG=org.firn.dnscheck
D=/sdcard/Android/data/$PKG/files
$ADB root >/dev/null 2>&1; sleep 1
$ADB shell am force-stop $PKG; $ADB uninstall $PKG >/dev/null 2>&1
$ADB install -r "$W/dnscheck.apk" >/dev/null || { echo "install failed"; exit 1; }
$ADB logcat -c
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
OUT=""
for i in $(seq 1 40); do
    OUT=$($ADB shell cat $D/stdout.txt 2>/dev/null | tr -d '\r')
    printf '%s' "$OUT" | grep -q '^DONE' && break
    sleep 2
done
echo "$OUT" | sed 's/^/        | /'
has "app finished (DONE)" "$OUT" '^DONE'
has "dnsconf ran (CONF resolv line)" "$OUT" '^CONF resolv [0-9]+ nameservers=[0-9]+'
has "dnsconf hosts line" "$OUT" '^CONF hosts [0-9]+'
for t in "8.8.8.8 1" "10.0.2.3 1" "256.1.1.1 0" "1.2.3 0" "1.2.3.4.5 0" "1..2.3 0" "a.b.c.d 0" "1.2.3.4x 0"; do
    has "prop_ipv4_ok $t" "$OUT" "^PROP ${t//./\\.}\$"
done
has "resolver has servers" "$OUT" '^SERVERS [1-9]'
has "localhost via the hosts table" "$OUT" '^LOCALHOST localhost OK 127\.0\.0\.1'
has "name -> address through the fake server" "$OUT" '^FAKE plain\.test OK 192\.0\.2\.1'
has "CNAME chain through the fake server" "$OUT" '^FAKE chain\.test OK 192\.0\.2\.2'
has "NXDOMAIN is an error" "$OUT" '^FAKE nx\.test ERR'
has "a silent server ends in an error, not a hang" "$OUT" '^FAKE silent\.test ERR'
if printf '%s\n' "$OUT" | grep -qE '^SYS example\.com OK '; then ok "the default resolver reaches the real network"
else echo "  note  default resolver: no answer for example.com (no route?): $(printf '%s\n' "$OUT" | grep '^SYS')"; fi
has "the fake server saw the questions" "$(tr -d '\0' <"$W/dns.log")" '^Q plain\.test 1 udp'
CRASH=$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|SIGABRT|ANR in '$PKG)
[ "$CRASH" = 0 ] && ok "no crash" || bad "no crash" "$CRASH crash lines in logcat"
$ADB shell am force-stop $PKG

echo "== 4. B: the push relay by NAME"
bash tools/android/build.sh demos/pushdemo/main.fi --name "Firn Push" --package org.firn.pushdemo --push \
    --abi "$BA" --args-file <(printf 'relay.test\n%s\n10.0.2.2:%s\n' "$RPORT" "$DPORT") \
    --out "$W/push.apk" >"$W/build_b.log" 2>&1 || { grep -v RWX "$W/build_b.log" | tail; exit 1; }
PKG=org.firn.pushdemo
$ADB shell am force-stop $PKG; $ADB uninstall $PKG >/dev/null 2>&1
$ADB install -r "$W/push.apk" >/dev/null || { echo "install failed"; exit 1; }
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS
$ADB logcat -c
: >"$W/dns.log"
$ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
for i in $(seq 1 30); do grep -q '^connected' "$W/relay.log" 2>/dev/null && break; sleep 1; done
has "the relay saw the app connect (name resolved on the push thread)" "$(cat "$W/relay.log")" '^connected'
has "the fake server was asked for relay.test (A, UDP)" "$(tr -d '\0' <"$W/dns.log")" '^Q relay\.test 1 udp'
sleep 3
has "the line arrived as a notification" \
    "$($ADB shell dumpsys notification --noredact | grep -c 'android.text=String (dns-check-line)' | tr -d '\0')" '^1$'
CFG=$($ADB shell cat /data/data/$PKG/files/firn-push.cfg 2>/dev/null | tr -d '\r\0')
has "firn-push.cfg keeps host, port and the name server" "$(printf '%s' "$CFG" | tr '\n' ' ')" "^relay\.test $RPORT 10\.0\.2\.2:$DPORT"
CRASH=$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|SIGABRT|ANR in '$PKG)
[ "$CRASH" = 0 ] && ok "no crash" || bad "no crash" "$CRASH crash lines in logcat"
$ADB shell am force-stop $PKG; $ADB uninstall $PKG >/dev/null 2>&1; $ADB uninstall org.firn.dnscheck >/dev/null 2>&1

# ---- C/D: TLS with a key pin and IPv6 ----------------------------------------
PKG=org.firn.pushdemo
NOTES() { $ADB shell dumpsys notification --noredact | grep -c "android.text=String ($1)" | tr -d '\0\r'; }
# push_app NAME HOST PORT PIN: build, install and start the pushdemo (PIN may be empty)
push_app() {
    local args; args=$(printf '%s\n%s\n10.0.2.2:%s\n' "$2" "$3" "$DPORT"); [ -n "$4" ] && args=$(printf '%s\n%s' "$args" "$4")
    bash tools/android/build.sh demos/pushdemo/main.fi --name "Firn Push" --package $PKG --push \
        --abi "$BA" --args-file <(printf '%s\n' "$args") --out "$W/$1.apk" >"$W/build_$1.log" 2>&1 \
        || { grep -v RWX "$W/build_$1.log" | tail; exit 1; }
    $ADB shell am force-stop $PKG; $ADB uninstall $PKG >/dev/null 2>&1
    $ADB install -r "$W/$1.apk" >/dev/null || { echo "install failed"; exit 1; }
    $ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS
    $ADB logcat -c
    : >"$W/dns.log"
    $ADB shell am start -n $PKG/android.app.NativeActivity >/dev/null
}
# tls_server PORT LINE: openssl s_server (P-256, TLS 1.3, ChaCha20-Poly1305); LINE is sent once a client is in
tls_server() {
    [ -n "$TLS_PID" ] && { kill "$TLS_PID" 2>/dev/null; wait "$TLS_PID" 2>/dev/null; }
    rm -f "$W/tin"; mkfifo "$W/tin"; : >"$W/tls.log"
    openssl s_server -accept "$1" -cert "$W/cert.pem" -key "$W/key.pem" -tls1_3 \
        -ciphersuites TLS_CHACHA20_POLY1305_SHA256 -no_ticket <"$W/tin" >"$W/tls.log" 2>&1 &
    TLS_PID=$!
    exec 7<>"$W/tin"
    TLS_LINE=$2
}
tls_send() { # wait for a client, then send the line
    for i in $(seq 1 40); do grep -q 'CIPHER is' "$W/tls.log" 2>/dev/null && break; sleep 1; done
    sleep 1; echo "$TLS_LINE" >&7
}
echo "== 5. C: TLS with a key pin (openssl s_server, P-256)"
openssl ecparam -name prime256v1 -genkey -noout -out "$W/key.pem" 2>/dev/null
openssl req -new -x509 -key "$W/key.pem" -subj /CN=relay.test -days 2 -out "$W/cert.pem" 2>/dev/null
PIN=$(openssl x509 -in "$W/cert.pem" -pubkey -noout | openssl pkey -pubin -outform der 2>/dev/null | sha256sum | cut -d' ' -f1)
BADPIN=$(printf 'not the key' | sha256sum | cut -d' ' -f1)
TPORT=${TLS_PORT:-7794}
echo "   pin $PIN  tls :$TPORT"
tls_server "$TPORT" tls-check-line
push_app tls relay.test "$TPORT" "$PIN"
tls_send
has "the TLS server saw a TLS 1.3 / ChaCha20 client" "$(cat "$W/tls.log")" 'CIPHER is TLS_CHACHA20_POLY1305_SHA256'
sleep 3
has "the pinned TLS line arrived as a notification" "$(NOTES tls-check-line)" '^1$'
LOG=$($ADB logcat -d -s firn-push)
has "log: connected (tls, pinned key)" "$LOG" 'connected \(tls, pinned key\)'
has "the fake server was asked for relay.test" "$(tr -d '\0' <"$W/dns.log")" '^Q relay\.test 1 udp'
CFG=$($ADB shell cat /data/data/$PKG/files/firn-push.cfg 2>/dev/null | tr -d '\r\0')
has "firn-push.cfg keeps the pin (4th line)" "$(printf '%s' "$CFG" | tr '\n' ' ')" "^relay\.test $TPORT 10\.0\.2\.2:$DPORT $PIN"
CRASH=$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|SIGABRT|ANR in '$PKG)
[ "$CRASH" = 0 ] && ok "no crash" || bad "no crash" "$CRASH crash lines in logcat"

echo "   -- a wrong pin"
tls_server "$TPORT" wrong-pin-line
push_app badpin relay.test "$TPORT" "$BADPIN"
tls_send
sleep 6
LOG=$($ADB logcat -d -s firn-push)
has "log: tls handshake failed" "$LOG" 'tls handshake failed'
if printf '%s\n' "$LOG" | grep -qF 'connected (tls'; then bad "a wrong pin must not connect" "$LOG"; else ok "a wrong pin never connects"; fi
has "a wrong pin shows no notification" "$(NOTES wrong-pin-line)" '^0$'
CRASH=$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|SIGABRT|ANR in '$PKG)
[ "$CRASH" = 0 ] && ok "no crash" || bad "no crash" "$CRASH crash lines in logcat"

echo "== 6. D: IPv6 (dual6.test: dead A, AAAA fec0::2)"
V6PORT=${V6_PORT:-7795}
python3 tools/android/relay_test.py "$V6PORT" --v6 --spacing 1 "v6-check-line" >"$W/relay6.log" 2>&1 &
V6_PID=$!
sleep 1
push_app v6 dual6.test "$V6PORT" ""
for i in $(seq 1 40); do grep -q '^connected' "$W/relay6.log" 2>/dev/null && break; sleep 1; done
has "the relay was reached over IPv6 (peer is ::1)" "$(cat "$W/relay6.log")" "^connected \('::1'"
has "the fake server was asked for dual6.test (A, then AAAA)" "$(tr -d '\0' <"$W/dns.log" | grep '^Q dual6' | awk '{print $3}' | sort -u | tr '\n' ' ')" '^1 28 $'
sleep 3
has "log: relay reached over ipv6" "$($ADB logcat -d -s firn-push)" 'relay reached over ipv6'
has "the IPv6 line arrived as a notification" "$(NOTES v6-check-line)" '^1$'
CRASH=$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|SIGABRT|ANR in '$PKG)
[ "$CRASH" = 0 ] && ok "no crash" || bad "no crash" "$CRASH crash lines in logcat"

echo "   -- TLS over IPv6"
tls_server "$TPORT" tls6-check-line
push_app tls6 dual6.test "$TPORT" "$PIN"
tls_send
sleep 3
LOG=$($ADB logcat -d -s firn-push)
has "log: relay reached over ipv6" "$LOG" 'relay reached over ipv6'
has "log: connected (tls, pinned key)" "$LOG" 'connected \(tls, pinned key\)'
has "the TLS line over IPv6 arrived as a notification" "$(NOTES tls6-check-line)" '^1$'
CRASH=$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|SIGSYS|SIGABRT|ANR in '$PKG)
[ "$CRASH" = 0 ] && ok "no crash" || bad "no crash" "$CRASH crash lines in logcat"
$ADB shell am force-stop $PKG; $ADB uninstall $PKG >/dev/null 2>&1

echo "ANDROID DNS: $PASS passed, $FAIL failed"
[ $FAIL -eq 0 ]
