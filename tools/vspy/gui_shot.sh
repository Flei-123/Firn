#!/bin/sh
# tools/vspy/gui_shot.sh <out.png> <title> <cmd...>
# Runs a GUI program under Xvfb, optionally drives it, and takes a screenshot.
# Steps come from $STEPS (xdotool commands separated by ';', window-relative
# "click X Y" and "type TEXT" and "sleep S"). Needs xvfb-run, xdotool, scrot.
out=$1; title=$2; shift 2
exec xvfb-run -a -s "-screen 0 800x600x24" sh -c '
out="$1"; title="$2"; shift 2
"$@" >/dev/null 2>&1 &
pid=$!
w=""
for i in $(seq 1 50); do
  w=$(xdotool search --name "$title" 2>/dev/null | head -1)
  [ -n "$w" ] && break
  sleep 0.2
done
[ -z "$w" ] && { echo "no window"; kill $pid; exit 1; }
sleep 0.5
eval "$(xdotool getwindowgeometry --shell "$w")"
echo "window ${WIDTH}x${HEIGHT} at $X,$Y"
echo "$STEPS" | tr ";" "\n" | while read -r cmd arg1 arg2; do
  case "$cmd" in
    click) xdotool mousemove $((X+arg1)) $((Y+arg2)) click 1; sleep 0.4;;
    type) xdotool type --delay 40 "$arg1"; sleep 0.4;;
    sleep) sleep "$arg1";;
  esac
done
scrot -o "$out"
kill $pid 2>/dev/null
' sh "$out" "$title" "$@"
