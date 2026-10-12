#!/usr/bin/env bash
# tools/gpu/win/wm_run.sh -- GL window under a REAL EWMH window manager (openbox) on Xvfb: wmprobe.fi (docs/GPU.md "EWMH").
#   * _NET_WM_STATE_FULLSCREEN on: the window grows to the screen, lib/window says is_fullscreen, and xprop / xwininfo (outside
#     views of the X server) agree; off: windowed again, the property is gone
#   * focus: the WM focuses the new window (xdotool getwindowfocus); xdotool moves the focus to a second window and back -> EV_FOCUS out / in
#   SKIP (exit 0) without Xvfb / openbox / xdotool / xprop / xwininfo / libEGL. Usage: bash tools/gpu/win/wm_run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
for c in Xvfb openbox xdotool xprop xwininfo; do command -v $c >/dev/null 2>&1 || { echo "   SKIP: $c is missing"; exit 0; }; done
[ -e /usr/lib/x86_64-linux-gnu/libEGL.so.1 ] || { echo "   SKIP: no libEGL"; exit 0; }
D=$(mktemp -d); XP=""; OB=""; P1=""; P2=""
trap 'for p in $P1 $P2 $OB $XP; do kill $p 2>/dev/null; done; rm -rf "$D"' EXIT
"$FIRNC" -o "$D/wm" tools/gpu/win/wmprobe.fi 2>"$D/err" || { echo "   FAIL: compile wmprobe.fi"; head -5 "$D/err"; exit 1; }
mkfifo "$D/disp"; Xvfb -displayfd 3 -screen 0 1024x768x24 -dpi 96 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 & XP=$!
read -r -t 20 n <"$D/disp" || { echo "   SKIP: Xvfb did not start"; exit 0; }
export DISPLAY=":$n"
HOME="$D" openbox --sm-disable >"$D/ob.txt" 2>&1 & OB=$!
for i in $(seq 1 50); do xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q "window id" && break; sleep 0.1; done
xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q "window id" || { echo "   SKIP: openbox did not come up"; exit 0; }
wmname=$(xprop -root -notype _NET_SUPPORTING_WM_CHECK | sed 's/.*# //'); wmname=$(xprop -id "$wmname" -notype _NET_WM_NAME 2>/dev/null | sed 's/.*= //')
echo "   window manager: $wmname"
fail=0
waitfor() { local i; for i in $(seq 1 $2); do grep -q "$1" "$D/main.txt" 2>/dev/null && return 0; sleep 0.2; done; return 1; }
timeout 60 "$D/wm" main >"$D/main.txt" 2>&1 & P1=$!
waitfor FS_HOLD 100 || { echo "   FAIL: no FS_HOLD"; cat "$D/main.txt"; exit 1; }
wid=$(xdotool search --name '^wmprobe$' | head -1)
st=$(xprop -id "$wid" _NET_WM_STATE 2>/dev/null)
geo=$(xwininfo -id "$wid" | sed -n 's/^  Width: \(.*\)/\1/p;s/^  Height: \(.*\)/x\1/p' | tr -d '\n')
foc=$(xdotool getwindowfocus 2>/dev/null)
[ "$foc" = "$wid" ] || { echo "   FAIL: the WM did not focus the new window (focus=$foc window=$wid)"; fail=1; }
echo "   outside view while full screen: $st | ${geo}"
echo "$st" | grep -q "_NET_WM_STATE_FULLSCREEN" && [ "$geo" = "1024x768" ] || { echo "   FAIL: X server does not show a full screen window"; fail=1; }
waitfor FS_OFF 60 || { echo "   FAIL: no FS_OFF"; fail=1; }
st2=$(xprop -id "$wid" _NET_WM_STATE 2>/dev/null)
echo "$st2" | grep -q "_NET_WM_STATE_FULLSCREEN" && { echo "   FAIL: property still set after leaving full screen: $st2"; fail=1; }
# focus: second window, move the focus there and back
waitfor FOCUS_READY 60 || { echo "   FAIL: no FOCUS_READY"; fail=1; }
timeout 20 "$D/wm" other >"$D/other.txt" 2>&1 & P2=$!
sleep 1.5
oid=$(xdotool search --name '^wmprobe-other$' | head -1)
if [ -n "$oid" ]; then
    xdotool windowfocus "$oid"; sleep 1; xdotool windowfocus "$wid"; sleep 1
else echo "   FAIL: second window not found"; fail=1; fi
wait "$P1"; rc=$?; P1=""
sed 's/^/   wm: /' "$D/main.txt" | grep -v "^   wm: \(FS_\|FOCUS_\)"
if [ "$rc" -eq 0 ] && grep -q "^RESULT fails=0" "$D/main.txt" && [ "$fail" -eq 0 ]; then
    echo "   ok:   EWMH full screen on/off + focus events under $wmname (outside view agrees)"
    echo "RESULT wm: ok"; exit 0
fi
echo "   FAIL: EWMH probe (exit $rc)"; echo "RESULT wm: FAIL"; exit 1
