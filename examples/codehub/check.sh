#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# examples/codehub/check.sh -- the CodeHub page, built and proven:
#   1. codehub.fi is what build.py writes (texts, theme, SVGs in sync)
#   2. native build; PNGs at 1996x1211 and 360x800 (and the whole phone
#      page); the scene's guards hold; frame time <= 16 ms at both sizes
#   3. the theme file passes fUi's contrast check (tools/fui/themefile_main)
#   4. WebAssembly build (size printed); if node + Playwright are there:
#      headless Chromium -- load time, frame time, hover, scroll, and the
#      browser's pixels against the native PNG (examples/codehub/check_web.cjs)
# Usage: bash examples/codehub/check.sh   (W= work directory, default /tmp/codehub-check)
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC=${FIRNC:-compiler/target/release/firnc}
W=${W:-/tmp/codehub-check}
mkdir -p "$W"
fail=0
echo "== 1. codehub.fi in sync with its sources =="
cp examples/codehub/codehub.fi "$W/codehub.before"
python3 examples/codehub/build.py >/dev/null
if cmp -s examples/codehub/codehub.fi "$W/codehub.before"; then echo "   OK"; else echo "   FAIL: run examples/codehub/build.py and commit"; fail=1; fi
echo "== 2. native =="
"$FIRNC" --opt-level=release-fast -o "$W/codehub" examples/codehub/main.fi || exit 1
for s in 1996x1211 360x800; do
    "$W/codehub" --png="$W/native-$s.png" --size=$s > "$W/png-$s.log" || { cat "$W/png-$s.log"; fail=1; }
    grep -q 'scene check (0 = ok): *0' "$W/png-$s.log" && echo "   $s: PNG, scene check 0" || { echo "   FAIL $s scene check"; fail=1; }
    "$W/codehub" --bench=60 --size=$s > "$W/bench-$s.log"
    rest=$(awk '/frame at rest, best/ {print $5}' "$W/bench-$s.log")
    full=$(awk '/frame full, best/ {print $4}' "$W/bench-$s.log")
    echo "   $s: frame at rest $rest ms, everything painted $full ms (best of 60)"
    awk -v m="$full" 'BEGIN { exit !(m <= 16.0) }' || { echo "   FAIL: over 16 ms"; fail=1; }
done
"$W/codehub" --png="$W/native-360-full.png" --size=360x800 --full > /dev/null || fail=1
# the still cache must not change a pixel
for s in 1996x1211 360x800; do
    "$W/codehub" --png="$W/native-$s-nostill.png" --size=$s --no-still > /dev/null || fail=1
    n=$(python3 -c "
from PIL import Image, ImageChops
a=Image.open('$W/native-$s.png').convert('RGB'); b=Image.open('$W/native-$s-nostill.png').convert('RGB')
print(sum(1 for p in ImageChops.difference(a,b).getdata() if max(p)>2))")
    [ "$n" = 0 ] && echo "   $s: still cache = everything painted (0 px differ)" || { echo "   FAIL $s: still cache differs in $n px"; fail=1; }
done
"$W/codehub" --png="$W/native-1996x1211-hover.png" --size=1996x1211 --hover=3 > /dev/null || fail=1
cmp -s "$W/native-1996x1211.png" "$W/native-1996x1211-hover.png" && { echo "   FAIL: hover changes nothing"; fail=1; } || echo "   hover on the red button changes the picture"
echo "== 3. the theme file =="
"$FIRNC" -o "$W/themefile" tools/fui/themefile_main.fi && "$W/themefile" examples/codehub/codehub.theme | sed 's/^/   /'
"$W/themefile" examples/codehub/codehub.theme | grep -q accepted || fail=1
echo "== 4. WebAssembly =="
"$FIRNC" --opt-level=release-fast --target=wasm32-browser -o examples/codehub/site/codehub.wasm examples/codehub/web.fi || exit 1
echo "   codehub.wasm: $(stat -c %s examples/codehub/site/codehub.wasm) octets"
PW=${PLAYWRIGHT:-}
[ -z "$PW" ] && for c in /root/jarvis/node_modules/playwright "$(npm root -g 2>/dev/null)/playwright"; do [ -d "$c" ] && PW=$c && break; done
if command -v node >/dev/null && [ -n "$PW" ]; then
    PLAYWRIGHT="$PW" node examples/codehub/check_web.cjs "$W/web" "$W/native-1996x1211.png" | sed 's/^/   /'
    [ "${PIPESTATUS[0]}" = 0 ] || fail=1
else
    echo "   SKIP: no node/Playwright -- the browser half is not checked"
fi
[ $fail = 0 ] && echo "CODEHUB PASSED" || echo "CODEHUB FAILED"
exit $fail
