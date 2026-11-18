#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
step_net() {
    echo "== fetch and SSE (server on 10.0.2.2:$PORT) =="
    check "the emulator network reaches the test server" wait_network
    echo "   (network ready after ${NET_WAITED}s)"
    $ADB logcat -c
    send origin "http://10.0.2.2:$PORT"
    send fetch /json
    expect "fetch GET /json: status 200 and the JSON body" 'status=200 body="{"hello":"firn","answer":42}"'
    send post /echo hello=firn
    expect "fetch POST /echo: the body reaches the server" 'status=200 body="echo:hello=firn"'
    check "the test server saw GET /json and POST /echo" "grep -q 'GET /json' $SERVER_LOG && grep -q 'POST /echo' $SERVER_LOG"
    send stream /events
    expect "SSE: the stream reports open" 'hostdemo sse_state open=1'
    expect "SSE: named event with its data" 'hostdemo sse name="hello" data="first"'
    expect "SSE: second event" 'data="second"'
    send streamclose
    expect "SSE: the stream reports closed" 'hostdemo sse_state open=0'
}

step_state() {
    echo "== store (across a restart) and clipboard =="
    $ADB logcat -c
    send storeset greeting persisted-value-42
    send storeget greeting
    expect "store: the value reads back" 'value="persisted-value-42"'
    restart_app
    send storeget greeting
    expect "store: the value is still there after an app restart" 'value="persisted-value-42"'
    send clipset clip from firn 123
    check "clipboard write returns true" '[ "$LAST_OK" = 1 ]'
    send clipget
    expect "clipboard read returns the written text" 'hostdemo clip status=1 text="clip from firn 123"'
}

step_typing() {
    echo "== typing: adb input text and imeinject =="
    $ADB logcat -c
    check "keyboard: the system input method is shown" show_keyboard
    $ADB shell input text hello
    sleep 2
    check "adb shell input text arrives as text events (\"hello\")" '[ "$(typed)" = hello ]'
    $ADB logcat -c
    ime commit 48 69
    check "imeinject commit arrives as text (\"Hi\")" '[ "$(typed)" = Hi ]'
    ime comp fc
    check "imeinject composing u-umlaut arrives as text" '[ "$(typed)" = "Hiü" ]'
    ime finish
    ime key 67
    expect "imeinject Backspace arrives as a Backspace key" 'hostdemo key name="Backspace"'
    send kbd off
    check "keyboard: hidden again" "wait_ime 0"
    $ADB logcat -c
    send edit on
    send kbd on
    wait_ime 1
    ime commit 48 69
    expect "edit mode: commit arrives as WE_EDIT with text and caret" 'hostdemo edit text="Hi" cur=2 anc=2'
    ime comp fc
    expect "edit mode: composing text with its range" 'hostdemo edit text="Hiü" cur=4 anc=4 ca=2 cb=4'
    send editset "set by the program"
    send kbd off
    send edit off
}

step_notify() {
    echo "== notify =="
    $ADB shell pm revoke $PKG android.permission.POST_NOTIFICATIONS >/dev/null 2>&1
    $ADB logcat -c
    send notify first-notification
    check "notify without the permission returns false" '[ "$LAST_OK" = 0 ]'
    check "notify asks for POST_NOTIFICATIONS itself (permission dialog opens)" "wait_focus 'GrantPermissions|permissioncontroller'"
    $ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS
    $ADB shell input keyevent KEYCODE_BACK
    sleep 1
    start_app
    send notify second-notification
    check "notify with the permission returns true" '[ "$LAST_OK" = 1 ]'
    sleep 1
    check "dumpsys notification lists the text" "$ADB shell dumpsys notification --noredact | grep -q second-notification"
}

step_save() {
    echo "== save =="
    $ADB shell rm -f /sdcard/Download/hostdemo.txt >/dev/null 2>&1
    send save hostdemo.txt saved-by-firn
    check "save returns true" '[ "$LAST_OK" = 1 ]'
    sleep 1
    check "/sdcard/Download/hostdemo.txt has the content" '[ "$($ADB shell cat /sdcard/Download/hostdemo.txt 2>/dev/null | tr -d "\r")" = saved-by-firn ]'
    $ADB shell rm -f /sdcard/Download/hostdemo.txt >/dev/null 2>&1
}

step_share_pick() {
    echo "== share and pick =="
    send share shared-by-firn
    check "share returns true" '[ "$LAST_OK" = 1 ]'
    check "share: the chooser opens" "wait_focus 'Chooser|Resolver|intentresolver'"
    $ADB shell input keyevent KEYCODE_BACK
    sleep 1
    start_app
    $ADB logcat -c
    send pick file
    check "pick returns true" '[ "$LAST_OK" = 1 ]'
    check "pick: the document picker opens" "wait_focus 'documentsui|DocumentsActivity|PickActivity'"
    $ADB shell input keyevent KEYCODE_BACK
    expect "pick: cancelling answers with status 0" 'hostdemo dialog status=0'
}

step_back_route() {
    echo "== back key, routes, insets =="
    start_app
    send backhold on
    $ADB shell input keyevent KEYCODE_BACK
    expect "back_hold on: the Back key arrives as WE_BACK" 'hostdemo back'
    send backhold off
    send route /a
    send route /b
    expect "navigate: WE_ROUTE with the new route" 'hostdemo route path="/b"'
    $ADB shell input keyevent KEYCODE_BACK
    expect "Back pops the route stack (/a)" 'hostdemo route path="/a"'
    send where
    expect "location returns the current route" 'hostdemo where path="/a"'
    $ADB shell input keyevent KEYCODE_BACK
    expect "Back pops to the start route" 'hostdemo route path="/"'
    $ADB logcat -c
    show_keyboard
    send insets
    check "insets returns true with four numbers" '[ "$LAST_OK" = 1 ] && [ -n "$(inset_bottom)" ]'
    check "the keyboard changes the layout (bottom inset or window resize)" keyboard_changes_layout
    send kbd off
    wait_ime 0
    sleep 1
    send insets
    $ADB shell input keyevent KEYCODE_BACK
    sleep 2
    check "Back at the start route hands over to the system (the app leaves the screen)" "! focus | grep -q $PKG"
}

step_crash() {
    echo "== crash check =="
    check "no crash in the log" "[ \$($ADB logcat -d | grep -cE 'FATAL|SIGSEGV|signal 11|Fatal signal') = 0 ]"
}

not_checked() {
    skipped "IME fine points (suggestions, autocorrect, gesture typing)" "needs a real device"
    skipped "push in the background (foreground service, doze)" "needs a real device"
    skipped "notification tap opening a route" "needs a real device"
}
