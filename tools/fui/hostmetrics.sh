#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/hostmetrics.sh -- W44-c: the host reads the system's safe area (insets) and text size in a real X11 window on an own Xvfb.
#   FUI_INSETS / FUI_TEXT_SCALE override; without them: _NET_WORKAREA (set with xprop) and gsettings text-scaling-factor (not testable
#   here, so only "no answer = 1000" is pinned). Own PIDs only (no pkill). Exit 77 = not testable here (no Xvfb).
cd "$(dirname "$0")/../.." || exit 1
FIRNC="${FIRNC:-firnc}"
command -v Xvfb >/dev/null 2>&1 || { echo "hostmetrics: no Xvfb -- not testable"; exit 77; }
D=$(mktemp -d)
XPID=""
trap '[ -n "$XPID" ] && kill "$XPID" 2>/dev/null; [ -n "$SP" ] && kill "$SP" 2>/dev/null; rm -rf "$D"' EXIT
FIRNLIB="$PWD/lib" "$FIRNC" --opt-level=dev -o "$D/hmlive" tools/fui/hostmetrics_live_main.fi || { echo "hostmetrics: build failed"; exit 1; }
Xvfb -noreset -displayfd 3 -screen 0 800x600x24 3>"$D/disp" >/dev/null 2>&1 &
XPID=$!
i=0
while [ ! -s "$D/disp" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
[ -s "$D/disp" ] || { echo "hostmetrics: Xvfb did not start -- not testable"; exit 77; }
DISP=":$(cat "$D/disp")"
BAD=0
run() { # <label> <FUI_TEXT_SCALE|-> <FUI_INSETS|-> <expected line>
    OUT=$(env -u FUI_TEXT_SCALE -u FUI_INSETS ${2:+FUI_TEXT_SCALE=$2} ${3:+FUI_INSETS=$3} DISPLAY="$DISP" timeout 40 "$D/hmlive" 2>&1 | grep '^HOSTMETRICS')
    if [ "$OUT" = "$4" ]; then echo "  ok    $1 ($OUT)"; else echo "  WRONG $1: got '$OUT', want '$4'"; BAD=$((BAD + 1)); fi
}
run "no switch, no WM: work area = screen, no text setting" "" "" "HOSTMETRICS text=1000 ins=0,0,0,0 called=1"
run "FUI_TEXT_SCALE=1.25" 1.25 "" "HOSTMETRICS text=1250 ins=0,0,0,0 called=1"
run "FUI_TEXT_SCALE=150 (percent)" 150 "" "HOSTMETRICS text=1500 ins=0,0,0,0 called=1"
run "FUI_INSETS=0,40,0,30" "" "0,40,0,30" "HOSTMETRICS text=1000 ins=0,40,0,30 called=1"
run "both switches" 2 "10,20,30,40" "HOSTMETRICS text=2000 ins=10,20,30,40 called=1"
run "FUI_INSETS garbage = refused = zero insets" "" "1,2,3" "HOSTMETRICS text=1000 ins=0,0,0,0 called=1"
# a panel: the work area starts 40 px down and ends 30 px above the bottom (800x600 screen) -> the window at 0,0 has a top inset of 40
# (the property is set with python-xlib; Xvfb needs -noreset, else the property is gone with its client). Skipped without python3-xlib.
if DISPLAY="$DISP" python3 -I -c "
from Xlib import display, Xatom
d = display.Display()
d.screen().root.change_property(d.intern_atom('_NET_WORKAREA'), Xatom.CARDINAL, 32, [0, 40, 800, 530])
d.sync()
" 2>/dev/null; then
    run "real _NET_WORKAREA 0,40,800,530" "" "" "HOSTMETRICS text=1000 ins=0,40,0,0 called=1"
    run "FUI_INSETS wins over the real work area" "" "5,6,7,8" "HOSTMETRICS text=1000 ins=5,6,7,8 called=1"
else
    echo "  skip  _NET_WORKAREA (no python3-xlib)"
fi
# W47-b (r386): TWO monitors (RandR 1.5 virtual monitors: xrandr --setmonitor) on the 800x600 screen: A = 0..399, B = 400..799. Bars are
# docks with _NET_WM_STRUT_PARTIAL / _NET_WM_STRUT in _NET_CLIENT_LIST (hostmetrics_struts.py). The work area is what a window manager
# would write: ONE rectangle for both monitors (top 40). A window 800 wide at 0,0 has its centre on B (its left inset 400 = A's width;
# the toolkit's UI scale makes it a bit wider than the screen, so the RIGHT inset is not pinned: normalised to R).
# Skipped without xrandr, python3-xlib or an X server that takes --setmonitor.
SP=""
cleanup_docks() { [ -n "$SP" ] && kill "$SP" 2>/dev/null; SP=""; rm -f "$D/ready"; }
docks() { # <spec>...  -> docks up, ready
    cleanup_docks
    DISPLAY="$DISP" python3 -I tools/fui/hostmetrics_struts.py "$D/ready" "$@" 2>/dev/null &
    SP=$!
    i=0
    while [ ! -s "$D/ready" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
    [ -s "$D/ready" ]
}
run_w() { # <label> <window width> <expected line>
    OUT=$(env -u FUI_TEXT_SCALE -u FUI_INSETS FUI_LIVE_W="$2" DISPLAY="$DISP" timeout 40 "$D/hmlive" 2>&1 | grep '^HOSTMETRICS' | sed 's/ins=\([0-9]*\),\([0-9]*\),[0-9]*,/ins=\1,\2,R,/')
    if [ "$OUT" = "$3" ]; then echo "  ok    $1 ($OUT)"; else echo "  WRONG $1: got '$OUT', want '$3'"; BAD=$((BAD + 1)); fi
}
if command -v xrandr >/dev/null 2>&1 \
    && DISPLAY="$DISP" xrandr --setmonitor MA 400/100x600/150+0+0 none >/dev/null 2>&1 \
    && DISPLAY="$DISP" xrandr --setmonitor MB 400/100x600/150+400+0 none >/dev/null 2>&1 \
    && DISPLAY="$DISP" python3 -I -c "
from Xlib import display, Xatom
d = display.Display()
r = d.screen().root
r.change_property(d.intern_atom('_NET_WORKAREA'), Xatom.CARDINAL, 32, [0, 40, 800, 560])
r.change_property(d.intern_atom('_NET_CLIENT_LIST'), Xatom.WINDOW, 32, [])
d.sync()
" 2>/dev/null; then
    run_w "2 monitors, no dock lists a strut: the work area as before (top 40 on A)" 200 "HOSTMETRICS text=1000 ins=0,40,R,0 called=1"
    run_w "2 monitors, no strut: window on B (centre 400)" 800 "HOSTMETRICS text=1000 ins=400,40,R,0 called=1"
    if docks "P:0,0,40,0,0,0,0,0,400,799,0,0"; then
        run_w "bar on B only (STRUT_PARTIAL span 400..799): window on A has none" 200 "HOSTMETRICS text=1000 ins=0,0,R,0 called=1"
        run_w "bar on B only: window on B has top 40" 800 "HOSTMETRICS text=1000 ins=400,40,R,0 called=1"
    else
        echo "  WRONG docks did not start"; BAD=$((BAD + 1))
    fi
    if docks "P:0,0,40,0,0,0,0,0,0,399,0,0"; then
        run_w "bar on A only: window on A has top 40" 200 "HOSTMETRICS text=1000 ins=0,40,R,0 called=1"
        run_w "bar on A only: window on B has none" 800 "HOSTMETRICS text=1000 ins=400,0,R,0 called=1"
    else
        echo "  WRONG docks did not start"; BAD=$((BAD + 1))
    fi
    if docks "S:0,0,40,0" "P:0,0,0,0,0,0,0,0,0,0,0,0"; then
        run_w "old _NET_WM_STRUT (whole edge) + an all-zero PARTIAL client: top 40 on A" 200 "HOSTMETRICS text=1000 ins=0,40,R,0 called=1"
    else
        echo "  WRONG docks did not start"; BAD=$((BAD + 1))
    fi
    cleanup_docks
else
    echo "  skip  2 monitors with struts (no xrandr --setmonitor / python3-xlib)"
fi
if [ "$BAD" -eq 0 ]; then echo "HOSTMETRICS LIVE PASSED"; exit 0; fi
echo "HOSTMETRICS LIVE FAILED ($BAD)"
exit 1
