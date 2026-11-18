#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
PKG=org.firn.hostdemo
ACTIVITY=$PKG/android.app.NativeActivity
CMD_PROP=debug.firn.hostdemo
IME_PROP=debug.firn.ime
LOG_POLLS=40
LOG_POLL_SECONDS=0.25
UI_POLLS=30
PASS=0
FAIL=0
SEQ=0
IME_SEQ=0
LAST_OK=0

ok() { echo "   OK      $1"; PASS=$((PASS + 1)); }
bad() { echo "   FAILED  $1"; FAIL=$((FAIL + 1)); }
skipped() { echo "   NOT CHECKED  $1 ($2)"; }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

app_log() { $ADB logcat -d -s firn:I 2>/dev/null | tr -d '\r' | grep -F 'hostdemo '; }

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
    local i=0
    while [ $i -lt $UI_POLLS ]; do
        [ "$(ime_shown)" = "$1" ] && return 0
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
