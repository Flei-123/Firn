#!/usr/bin/env bash
# tools/input/apphost_run.sh -- r444b: the REAL fUi window (lib/@linux/fui/apphost.fi, tools/input/fui_ime_main.fi) on a private Xvfb,
# keys and clicks sent by xdotool, XMODIFIERS=@im=none (Xlib's built-in input method: dead keys + Compose, no pre-edit callbacks).
#   (a program that does not import plat.fuiime stays a static ELF; the IME is opt-in)
#   typing -> fUi field; dead key / Compose key through the input method; NO double text;
#   pre-edit text + commit through the textcore -> fuitext -> field path (F5 plays the part of an IME: the test program feeds the
#   same textcore events a real IME's callbacks do); a mouse click while pre-edit text stands: the field keeps the text, the
#   input method's pending dead key is forgotten (XmbResetIC); FUI_NO_IME=1 = the old EV_CHAR path.
# NOT proven: a real IME (ibus / fcitx), a window manager, a real keyboard. Usage: bash tools/input/apphost_run.sh [firnc]
set -u
cd "$(dirname "$0")/../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
for c in xdotool Xvfb; do command -v $c >/dev/null || { echo "   SKIP: $c missing (apphost IME test not run)"; exit 0; }; done
D=$(mktemp -d)
XPID=""; APID=""
trap '[ -n "$APID" ] && kill "$APID" 2>/dev/null; [ -n "$XPID" ] && kill "$XPID" 2>/dev/null; rm -rf "$D"' EXIT
fail=0
ok()  { echo "   ok:   $1"; }
bad() { echo "   FAIL: $1"; fail=1; }
"$FIRNC" -o "$D/app" tools/input/fui_ime_main.fi 2>"$D/err" || { echo "   FAIL: build"; head -5 "$D/err"; exit 1; }
Xvfb -displayfd 3 -screen 0 800x600x24 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 &
XPID=$!
for _ in $(seq 1 50); do [ -s "$D/disp" ] && break; sleep 0.1; done
[ -s "$D/disp" ] || { echo "   FAIL: Xvfb did not start"; exit 1; }
export DISPLAY=":$(head -1 "$D/disp")"
# field text of node $1 after its last edit (empty if never edited)
field() { grep "^EDIT $1 " "$D/out" | tail -1 | sed -e "s/^EDIT $1 //" -e 's/|pre=.*$//'; }
launch() { # $1 = extra env assignment
    : >"$D/out"
    env XMODIFIERS=@im=none LANG=C.UTF-8 LC_ALL=C.UTF-8 $1 "$D/app" >"$D/out" 2>"$D/err" &
    APID=$!
    for _ in $(seq 1 50); do xdotool search --name "ime test" >/dev/null 2>&1 && break; sleep 0.1; done
    sleep 0.6
}
stop() { kill "$APID" 2>/dev/null; wait "$APID" 2>/dev/null; APID=""; }
click1() { xdotool mousemove 200 79 click 1; sleep 0.3; }
click2() { xdotool mousemove 200 127 click 1; sleep 0.3; }
key() { xdotool key "$@"; sleep 0.35; }

# a fUi program that did NOT opt in stays a static image (apphost must not import plat.fuitext / std.dynlib)
if "$FIRNC" -o "$D/form" examples/fui/form.fi 2>"$D/err"; then
    if command -v readelf >/dev/null && readelf -l "$D/form" 2>/dev/null | grep -q INTERP; then bad "plain fUi program became dynamically linked"
    else ok "plain fUi program (no fuiime) is still a static image"; fi
else bad "examples/fui/form.fi build"; fi
launch ""
click1
xdotool type --delay 60 "abc"; sleep 0.4
[ "$(field 1)" = "abc" ] && ok "typing 'abc' into the first fUi field (no double text)" || bad "typing: got '$(field 1)'"
grep -q "ime=on" "$D/out" && ok "input method open in the fUi window (ft_open)" || bad "input method not on"
key dead_acute; key e
[ "$(field 1)" = "abcé" ] && ok "dead key acute + e -> é" || bad "dead key: got '$(field 1)'"
key Multi_key; key o; key c
[ "$(field 1)" = "abcé©" ] && ok "Compose o c -> ©" || bad "compose: got '$(field 1)'"
key F5
[ "$(field 1)" = "abcé©ka" ] && ok "pre-edit text 'ka' stands in the field" || bad "pre-edit: got '$(field 1)'"
key F5
[ "$(field 1)" = "abcé©kana" ] && ok "pre-edit text replaced in full ('kana')" || bad "pre-edit 2: got '$(field 1)'"
key F5
[ "$(field 1)" = "abcé©KANJI" ] && ok "commit replaces the pre-edit text" || bad "commit: got '$(field 1)'"
# mouse click while pre-edit text stands: the field keeps it, the dead key waiting in the input method is forgotten
key F5
key dead_acute
click2
[ "$(field 1)" = "abcé©KANJIka" ] && ok "click during pre-edit: field 1 keeps the pre-edit text" || bad "click confirm: got '$(field 1)'"
xdotool type --delay 60 "e"; sleep 0.4
[ "$(field 2)" = "e" ] && ok "click reset the input method: 'e' after the lost dead key is a plain e, in field 2" || bad "click reset: got '$(field 2)'"
key F5
key F5
[ "$(field 2)" = "eKANJI" ] && ok "pre-edit replaced + committed in the newly focused field 2" || bad "field 2 pre-edit: got '$(field 2)'"
key F5
key Tab
[ "$(field 2)" = "eKANJIka" ] && ok "focus change (Tab) during pre-edit: the old field keeps the text" || bad "tab: got '$(field 2)'"
stop

# FUI_NO_IME: the old EV_CHAR path
launch "FUI_NO_IME=1"
click1
xdotool type --delay 60 "xyz"; sleep 0.4
[ "$(field 1)" = "xyz" ] && ok "FUI_NO_IME=1: plain typing as before" || bad "no-ime typing: got '$(field 1)'"
grep -q "ime=off" "$D/out" && ok "FUI_NO_IME=1: input method off" || bad "no-ime flag ignored"
stop
[ "$fail" -eq 0 ] && echo "RESULT apphost-ime: ok" || echo "RESULT apphost-ime: FAIL"
exit "$fail"
