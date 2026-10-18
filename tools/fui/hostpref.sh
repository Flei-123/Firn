#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/hostpref.sh -- W40-a: the host reads the system's transparency / high-contrast switch and the theme follows (real X11 window
# on an own Xvfb). FUI_TRANSPARENCY=0 -> glass flat; FUI_CONTRAST=1 -> High Contrast theme (black ground) + glass flat; =0 -> normal.
# Own PIDs only (no pkill). Exit 77 = not testable here (no Xvfb).
cd "$(dirname "$0")/../.." || exit 1
FIRNC="${FIRNC:-firnc}"
command -v Xvfb >/dev/null 2>&1 || { echo "hostpref: no Xvfb -- not testable"; exit 77; }
D=$(mktemp -d)
XPID=""
trap '[ -n "$XPID" ] && kill "$XPID" 2>/dev/null; rm -rf "$D"' EXIT
FIRNLIB="$PWD/lib" "$FIRNC" --opt-level=dev -o "$D/hostpref" tools/fui/hostpref_main.fi || { echo "hostpref: build failed"; exit 1; }
Xvfb -displayfd 3 -screen 0 800x600x24 3>"$D/disp" >/dev/null 2>&1 &
XPID=$!
i=0
while [ ! -s "$D/disp" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
[ -s "$D/disp" ] || { echo "hostpref: Xvfb did not start -- not testable"; exit 77; }
DISP=":$(cat "$D/disp")"
BAD=0
run() { # <label> <transparency|-> <contrast|-> <expected line>
    T="$2"; C="$3"
    OUT=$(env -u FUI_TRANSPARENCY -u FUI_CONTRAST ${T:+FUI_TRANSPARENCY=$T} ${C:+FUI_CONTRAST=$C} DISPLAY="$DISP" timeout 40 "$D/hostpref" 2>&1 | grep '^HOSTPREF')
    if [ "$OUT" = "$4" ]; then echo "  ok    $1 ($OUT)"; else echo "  WRONG $1: got '$OUT', want '$4'"; BAD=$((BAD + 1)); fi
}
BASE=$(env -u FUI_TRANSPARENCY -u FUI_CONTRAST FUI_TRANSPARENCY=1 FUI_CONTRAST=0 DISPLAY="$DISP" timeout 40 "$D/hostpref" 2>&1 | grep '^HOSTPREF' | sed 's/.*base=//')
[ -n "$BASE" ] && [ "$BASE" != "0" ] || { echo "  WRONG the normal theme has no ground colour ('$BASE')"; BAD=$((BAD + 1)); }
run "FUI_TRANSPARENCY=0: glass flat" 0 0 "HOSTPREF reduced=1 hc=0 base=$BASE"
run "FUI_TRANSPARENCY=1: glass on" 1 0 "HOSTPREF reduced=0 hc=0 base=$BASE"
run "FUI_CONTRAST=1: High Contrast theme, glass flat even with transparency on" 1 1 "HOSTPREF reduced=1 hc=1 base=0"
run "FUI_CONTRAST=0: normal theme" 1 0 "HOSTPREF reduced=0 hc=0 base=$BASE"
if [ "$BAD" -eq 0 ]; then echo "HOSTPREF PASSED"; exit 0; fi
echo "HOSTPREF FAILED ($BAD)"
exit 1
