#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/android/dismiss_anr.sh -- source this file; defines dismiss_anr: taps away the emulator's own "<x> isn't responding"
# dialogs (System UI / Launcher / Gboard on a cold, loaded emulator). They are no app fault, but they cover the app and
# make OCR checks fail. Needs $ADB (adb with -s serial). Returns 0 when a dialog was dismissed, 1 when there was none.
dismiss_anr() {
    local xml b i=0 hit=1
    while [ $i -lt 4 ]; do
        i=$((i+1))
        timeout 20 $ADB shell uiautomator dump /sdcard/anr_ui.xml >/dev/null 2>&1 || { sleep 1; continue; }
        xml=$(timeout 10 $ADB shell cat /sdcard/anr_ui.xml 2>/dev/null | tr '>' '\n')
        echo "$xml" | grep -q -E "text=\"[^\"]*(isn't responding|nicht reagiert|keeps stopping)" || break  # english: ok (German system dialog text)
        # prefer "Wait" (keeps the process), else "Close app"; German UI: "Warten"
        b=$(echo "$xml" | grep -E 'text="(Wait|Warten)"' | grep -oP 'bounds="\K[^"]+' | head -1)
        [ -n "$b" ] || b=$(echo "$xml" | grep -E 'text="(Close app|App schließen|OK)"' | grep -oP 'bounds="\K[^"]+' | head -1)
        [ -n "$b" ] || break
        $ADB shell input tap $(echo "$b" | tr '[],' '   ' | awk '{print int(($1+$3)/2), int(($2+$4)/2)}')
        echo "  (emulator ANR dialog dismissed)"; hit=0; sleep 2
    done
    $ADB shell rm -f /sdcard/anr_ui.xml >/dev/null 2>&1
    return $hit
}
