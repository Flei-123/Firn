#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
PKG=org.firn.hostdemo
ACTIVITY=$PKG/android.app.NativeActivity
CMD_PROP=debug.firn.hostdemo
IME_PROP=debug.firn.ime
LOG_POLLS=40
LOG_POLL_SECONDS=0.25
UI_POLLS=30
NET_POLLS=90
NET_POLL_SECONDS=1
NET_PROBE_SECONDS=2
CONTEXT_LINES=12
NET_WAITED=0
KBD_ATTEMPTS=4
KBD_ATTEMPT_POLLS=8
PASS=0
FAIL=0
SEQ=0
IME_SEQ=0
LAST_OK=0

ok() { echo "   OK      $1"; PASS=$((PASS + 1)); }
bad() { echo "   FAILED  $1"; FAIL=$((FAIL + 1)); fail_context; }
skipped() { echo "   NOT CHECKED  $1 ($2)"; }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

app_log() { $ADB logcat -d -s firn:I 2>/dev/null | tr -d '\r' | grep -F 'hostdemo '; }

fail_context() {
    echo "           focus: $(focus | cut -c1-120)"
    echo "           ime:   $($ADB shell dumpsys input_method 2>/dev/null | grep -m1 -o 'mInputShown=[a-z]*')"
    app_log | tail -$CONTEXT_LINES | sed 's/^/           app:    /'
    [ ! -s "${SERVER_LOG:-}" ] || tail -$CONTEXT_LINES "$SERVER_LOG" | sed 's/^/           server: /'
    $ADB logcat -d -b events 2>/dev/null | grep -E 'am_anr' | tail -2 | cut -c1-160 | sed 's/^/           /'
    [ -z "${SHOT_DIR:-}" ] || $ADB exec-out screencap -p >"$SHOT_DIR/fail_$FAIL.png" 2>/dev/null
    [ -z "${SHOT_DIR:-}" ] || $ADB logcat -d >"$SHOT_DIR/fail_$FAIL.log" 2>/dev/null
    [ -z "${SHOT_DIR:-}" ] || $ADB shell dumpsys input_method >"$SHOT_DIR/fail_$FAIL.ime" 2>/dev/null
}

host_reachable() {
    $ADB shell "printf 'GET /ping HTTP/1.0\\r\\n\\r\\n' | nc -w $NET_PROBE_SECONDS 10.0.2.2 $PORT" 2>/dev/null | grep -q '^HTTP/'
}

wait_network() {
    NET_WAITED=0
    while [ $NET_WAITED -lt $NET_POLLS ]; do
        host_reachable && return 0
        sleep $NET_POLL_SECONDS
        NET_WAITED=$((NET_WAITED + 1))
    done
    return 1
}

wait_log() {
    local i=0
    while [ $i -lt $LOG_POLLS ]; do
        app_log | grep -qF -- "$1" && return 0
        sleep $LOG_POLL_SECONDS
        i=$((i + 1))
    done
    return 1
}

expect() { check "$1" "wait_log '$2'"; }

send() {
    SEQ=$((SEQ + 1))
    $ADB shell setprop $CMD_PROP "\"$SEQ $*\""
    wait_log " cmd seq=$SEQ " || return 1
    LAST_OK=0
    app_log | grep -F " cmd seq=$SEQ " | tail -1 | grep -q ' ok=1$' && LAST_OK=1
    return 0
}

ime() {
    IME_SEQ=$((IME_SEQ + 1))
    $ADB shell setprop $IME_PROP "\"$IME_SEQ $*\""
    local i=0
    while [ $i -lt $LOG_POLLS ]; do
        $ADB logcat -d -s firn:I 2>/dev/null | grep -q "imeinject seq=$IME_SEQ ok=1" && break
        sleep $LOG_POLL_SECONDS
        i=$((i + 1))
    done
    sleep 1
}

typed() { app_log | grep -F 'hostdemo text value=' | sed 's/.*value="//; s/"$//' | tr -d '\n'; }

focus() { $ADB shell dumpsys window 2>/dev/null | grep -m1 mCurrentFocus | tr -d '\r'; }

wait_focus() {
    local i=0
    while [ $i -lt $UI_POLLS ]; do
        focus | grep -qE "$1" && return 0
        sleep 0.5
        i=$((i + 1))
    done
    return 1
}

ime_shown() { $ADB shell dumpsys input_method 2>/dev/null | grep -c 'mInputShown=true'; }

wait_ime() {
    local i=0 polls=${2:-$UI_POLLS}
    while [ $i -lt $polls ]; do
        [ "$(ime_shown)" = "$1" ] && return 0
        sleep 0.5
        i=$((i + 1))
    done
    return 1
}

show_keyboard() {
    local attempt=1
    while [ $attempt -le $KBD_ATTEMPTS ]; do
        send kbd on
        wait_ime 1 $KBD_ATTEMPT_POLLS && return 0
        echo "   (keyboard request $attempt ignored: the window has no input focus yet; asking again)"
        send kbd off
        attempt=$((attempt + 1))
    done
    return 1
}

keyboard_changes_layout() {
    local i=0
    while [ $i -lt $UI_POLLS ]; do
        send insets
        [ "$(inset_bottom)" -gt 0 ] 2>/dev/null && return 0
        [ "$(resizes)" -ge 1 ] && return 0
        sleep 0.5
        i=$((i + 1))
    done
    return 1
}

start_app() {
    $ADB shell setprop $CMD_PROP '""'
    $ADB logcat -c
    $ADB shell am start -n $ACTIVITY >/dev/null
    wait_log 'hostdemo ready' && wait_focus "$PKG"
}

restart_app() {
    $ADB shell am force-stop $PKG
    sleep 1
    start_app
}

inset_bottom() { app_log | grep -F 'hostdemo insets' | tail -1 | sed 's/.*bottom=//'; }

resizes() { app_log | grep -cF 'hostdemo resize'; }
