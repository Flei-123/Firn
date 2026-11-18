#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
SDK=${SDK:-/root/android-sdk}
AVD=${AVD:-screenf}
AVD_TEMPLATE=${AVD_TEMPLATE:-a31e}
EMU_PORT=${EMU_PORT:-5588}
ADB_PORT=${ADB_PORT:-5095}
BOOT_POLLS=200
BOOT_POLL_SECONDS=3
SCREEN_SIZE=1080x2400
SCREEN_DENSITY=420
ADBX="$SDK/platform-tools/adb -P $ADB_PORT"
SER=emulator-$EMU_PORT
ADB="$ADBX -s $SER"
AVDHOME=$HOME/.android/avd
EMU_WORK=""
EMU_PID=""

emu_wait_boot() {
    for _ in $(seq $BOOT_POLLS); do
        [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && return 0
        sleep $BOOT_POLL_SECONDS
    done
    return 1
}

emu_start() {
    EMU_WORK=$(mktemp -d)
    mkdir -p "$AVDHOME/$AVD.avd"
    sed -e "s/$AVD_TEMPLATE/$AVD/g" "$AVDHOME/$AVD_TEMPLATE.avd/config.ini" >"$AVDHOME/$AVD.avd/config.ini"
    printf 'avd.ini.encoding=UTF-8\npath=%s/%s.avd\npath.rel=avd/%s.avd\ntarget=android-35\n' "$AVDHOME" "$AVD" "$AVD" >"$AVDHOME/$AVD.ini"
    $ADBX start-server >/dev/null 2>&1
    ANDROID_ADB_SERVER_PORT=$ADB_PORT ANDROID_SDK_ROOT=$SDK ANDROID_AVD_HOME=$AVDHOME \
        "$SDK/emulator/emulator" -avd $AVD -port $EMU_PORT -no-window -no-audio -no-boot-anim -no-snapshot \
        -wipe-data -gpu swiftshader_indirect >"$EMU_WORK/emu.log" 2>&1 &
    EMU_PID=$!
    emu_wait_boot
    $ADB shell wm size $SCREEN_SIZE >/dev/null
    $ADB shell wm density $SCREEN_DENSITY >/dev/null
}

emu_stop() {
    $ADB emu kill >/dev/null 2>&1; sleep 2; kill $EMU_PID 2>/dev/null; $ADBX kill-server >/dev/null 2>&1
    rm -rf "$AVDHOME/$AVD.avd" "$AVDHOME/$AVD.ini" "$EMU_WORK"
}
