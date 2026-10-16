#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/cliphist/app_check.sh -- the clipboard history app (examples/cliphist) in a REAL window on X11 (private Xvfb +
# openbox) with a FOREIGN clipboard owner (Tk, tools/cliphist/foreign_owner.py): the owner copies, the window fills,
# a click / Enter puts an entry back (read with a second Tk process), Pin, X, Delete, Esc, Ctrl+Shift+V hide/show,
# duplicates, the history file (0600, restart keeps the history). Needs Xvfb, openbox, xdotool, xwd, xwininfo, python3
# with tkinter. Skips when one is missing.
#   FIRNC=<firnc> APP_SHOTS=<dir> bash tools/cliphist/app_check.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"; [ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
for t in Xvfb openbox xdotool xwd xwininfo python3; do
    command -v "$t" >/dev/null || { echo "skip: $t missing"; exit 0; }
done
python3 -c "import tkinter" 2>/dev/null || { echo "skip: python3 tkinter missing"; exit 0; }
WORK=$(mktemp -d)
PIDS=""
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null; done; rm -rf "$WORK"; }
trap cleanup EXIT
FAIL=0; N=0
ok() { N=$((N + 1)); echo "  ok   $1"; }
bad() { N=$((N + 1)); echo "  FAIL $1"; FAIL=$((FAIL + 1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }
OWNER="python3 -I tools/cliphist/foreign_owner.py"

"$FIRNC" --opt-level="${APP_LEVEL:-dev-fast}" -o "$WORK/app" examples/cliphist/main.fi 2>&1 | grep -v RWX
[ -x "$WORK/app" ] || { echo "build failed"; exit 1; }

D=$((300 + RANDOM % 100))
Xvfb ":$D" -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
PIDS="$PIDS $!"
sleep 1
export DISPLAY=":$D" HOME="$WORK/home" FIRN_CLIPBOARD=native
mkdir -p "$HOME"
openbox >/dev/null 2>&1 &
PIDS="$PIDS $!"
sleep 1

HIST="$WORK/hist.fch"
shot() {
    xwd -root -silent -out "$WORK/$1.xwd"
    if [ -n "${APP_SHOTS:-}" ]; then
        mkdir -p "$APP_SHOTS"
        python3 tools/fui/xwd2png.py "$WORK/$1.xwd" "$APP_SHOTS/$1.png" >/dev/null 2>&1
    fi
}
# changed pixels between two screenshots
diffpx() {
    python3 -I - "$WORK/$1.xwd" "$WORK/$2.xwd" <<'PY'
import sys
a, b = open(sys.argv[1], "rb").read(), open(sys.argv[2], "rb").read()
print(sum(1 for i in range(0, min(len(a), len(b)), 4) if a[i:i + 4] != b[i:i + 4]))
PY
}
find_win() {
    for _ in $(seq 100); do
        W=$(xdotool search --onlyvisible --name '^Clipboard$' 2>/dev/null | head -1)
        [ -n "$W" ] && return 0
        sleep 0.1
    done
    return 1
}
geom() { xwininfo -id "$W" | awk -v k="$1" '$0 ~ k {print $NF}' | head -1; }
# click at (dx, dy) inside the window
click() { xdotool mousemove $((WX + $1)) $((WY + $2)) click 1; sleep 0.5; }
hist_has() { grep -aq -- "$1" "$HIST" 2>/dev/null; }
wait_hist() { for _ in $(seq 40); do hist_has "$1" && return 0; sleep 0.1; done; return 1; }
wait_nohist() { for _ in $(seq 40); do hist_has "$1" || return 0; sleep 0.1; done; return 1; }
clip() { timeout 10 $OWNER read 2>/dev/null; }
visible() { xwininfo -id "$W" 2>/dev/null | grep -q IsViewable; }

echo "== 1. start, foreign owner copies =="
"$WORK/app" "$HIST" > "$WORK/app.out" 2>&1 &
APP=$!
PIDS="$PIDS $APP"
find_win || { bad "window appears"; cat "$WORK/app.out"; exit 1; }
sleep 0.8
ok "window appears ($W)"
WX=$(geom "Absolute upper-left X"); WY=$(geom "Absolute upper-left Y")
xdotool windowfocus "$W" 2>/dev/null
shot 1_empty
$OWNER 1 alpha beta gamma > "$WORK/owner.out" 2>&1 &
PIDS="$PIDS $!"
check "alpha taken in" 'wait_hist alpha'
check "beta taken in" 'wait_hist beta'
check "gamma taken in" 'wait_hist gamma'
sleep 0.8
shot 2_filled
PX=$(diffpx 1_empty 2_filled)
check "the rows appear in the window ($PX changed pixels)" '[ "$PX" -gt 1500 ]'
check "history file mode 0600" '[ "$(stat -c %a "$HIST")" = 600 ]'
sleep 2

echo "== 2. click a row =="
# rows (top to bottom): gamma, beta, alpha; row r: y = 69 + 50 r; pick x = 126, pin x = 284, X x = 346
click 126 169
check "click on 'alpha' puts it back" '[ "$(clip)" = alpha ]'
sleep 0.5
check "window hides after the pick" '! visible'
sleep 1
check "alpha not stored twice" '[ "$(grep -ao alpha "$HIST" | wc -l)" = 1 ]'

echo "== 3. hotkey =="
xdotool key ctrl+shift+v; sleep 0.8
check "Ctrl+Shift+V shows" 'visible'
shot 3_shown
xdotool key ctrl+shift+v; sleep 0.8
check "Ctrl+Shift+V hides" '! visible'
xdotool key ctrl+shift+v; sleep 0.8
check "Ctrl+Shift+V shows again" 'visible'
xdotool windowfocus "$W" 2>/dev/null; sleep 0.3

echo "== 4. keys =="
# order now: alpha (just used), gamma, beta
xdotool key Down; sleep 0.3
xdotool key Return; sleep 0.6
check "Down + Enter puts 'gamma' back" '[ "$(clip)" = gamma ]'
check "window hides after Enter" '! visible'
xdotool key ctrl+shift+v; sleep 0.8
xdotool windowfocus "$W" 2>/dev/null; sleep 0.3
xdotool key Escape; sleep 0.6
check "Esc hides" '! visible'

echo "== 5. pin, delete =="
xdotool key ctrl+shift+v; sleep 0.8
xdotool windowfocus "$W" 2>/dev/null; sleep 0.3
shot 5_before
# order now: gamma, alpha, beta; pin 'beta' (row 2)
click 284 169
shot 5_pinned
PX=$(diffpx 5_before 5_pinned)
check "Pin changes the row ($PX changed pixels)" '[ "$PX" -gt 300 ]'
# the pinned entry is first now: X on row 1 removes 'gamma' or 'alpha'; remove row 0 with the key: Delete removes the selected
click 346 119
sleep 1.2
CNT=0; for w in alpha beta gamma; do hist_has "$w" && CNT=$((CNT + 1)); done
check "X removes one entry from the history file (left: $CNT of 3)" '[ "$CNT" = 2 ]'
check "pinned 'beta' is still there" 'hist_has beta'

echo "== 6. restart keeps the history =="
kill "$APP" 2>/dev/null; wait "$APP" 2>/dev/null
shot 6_gone
"$WORK/app" "$HIST" > "$WORK/app2.out" 2>&1 &
APP2=$!
PIDS="$PIDS $APP2"
find_win || bad "window appears again"
sleep 1.2
shot 6_again
check "second start shows the stored entries" '[ "$(diffpx 6_gone 6_again)" -gt 1500 ]'
check "history file still there" 'hist_has beta'
kill "$APP2" 2>/dev/null

echo "== 7. a foreign owner with extra formats (python-xlib) =="
if python3 -I -c "import Xlib" 2>/dev/null; then
    XO="python3 -I tools/cliphist/xowner.py"
    rm -f "$HIST"
    "$WORK/app" "$HIST" > "$WORK/app4.out" 2>&1 &
    APP4=$!
    PIDS="$PIDS $APP4"
    sleep 1.5
    $XO 2 "text=plain1" "html=<b>bold</b>" > "$WORK/xo1.out" 2>&1 &
    PIDS="$PIDS $!"
    check "text + html owner: text stored" 'wait_hist plain1'
    check "text + html owner: html stored" 'wait_hist "<b>bold</b>"'
    sleep 2.5
    $XO 2 "text=hunter2" "hint=secret" > "$WORK/xo2.out" 2>&1 &
    PIDS="$PIDS $!"
    sleep 3.5
    check "password-manager hint 'secret': NOT in the history file" '! hist_has hunter2'
    $XO 2 "text=kept2" "hint=" > "$WORK/xo3.out" 2>&1 &
    PIDS="$PIDS $!"
    check "hint without 'secret': kept" 'wait_hist kept2'
    kill "$APP4" 2>/dev/null
else
    echo "  skip: python-xlib missing"
fi

echo "== 8. without a file nothing is written =="
rm -f "$HIST"
"$WORK/app" > "$WORK/app3.out" 2>&1 &
APP3=$!
PIDS="$PIDS $APP3"
sleep 1.5
$OWNER 1 secret-text > /dev/null 2>&1 &
PIDS="$PIDS $!"
sleep 3
check "no history file appears" '[ ! -e "$HIST" ] && [ -z "$(find "$WORK/home" "$WORK" -maxdepth 3 -name "*.fch" 2>/dev/null)" ]'
kill "$APP3" 2>/dev/null

echo "$N checks"
[ $FAIL -eq 0 ] && echo "CLIPHIST APP PASSED" || echo "CLIPHIST APP FAILED ($FAIL)"
exit $((FAIL > 0))
