#!/usr/bin/env bash
# tools/mc/hud/run.sh -- cross-check of lib/mc/mc_hud.fi against an independent python implementation (tools/mc/hud/ref.py):
# 18 scenes (all glyphs at scale 1..3, alpha blending + clipping, crosshair, hotbar, debug lines with number formatting,
# whole frames) must be BYTE identical (framebuffer + SHA-256). Built in 3 stages (release / --no-opt / dev-fast), all three
# must agree. The generated font (lib/mc/mc_hud_font.fi) must match font5x7.txt. Counter-checks: a damaged blend and a damaged
# font row must be detected.
#
# Usage: bash tools/mc/hud/run.sh [firnc-binary]
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
if python3 -I tools/mc/hud/gen_font.py tools/mc/hud/font5x7.txt "$W/font.fi" && cmp -s "$W/font.fi" lib/mc/mc_hud_font.fi; then
    echo "  ok    lib/mc/mc_hud_font.fi is generated from font5x7.txt"
else
    echo "  FAIL  lib/mc/mc_hud_font.fi differs from tools/mc/hud/gen_font.py output"; ERR=1
fi
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    mkdir -p "$W/$name"
    if ! $FIRNC $stage -o "$W/dump.$name" tools/mc/hud/dump.fi 2>"$W/err"; then
        echo "  FAIL  $name: dump.fi does not compile"; head -10 "$W/err"; ERR=1; continue
    fi
    if ! "$W/dump.$name" "$W/$name" 2>"$W/stderr"; then
        echo "  FAIL  $name: dump crashed"; head -5 "$W/stderr"; ERR=1; continue
    fi
    if [ "$name" = "release-fast" ]; then
        if python3 -I tools/mc/hud/ref.py "$W/$name" > "$W/out.txt" 2>&1; then
            cat "$W/out.txt"
        else
            echo "  FAIL  python reference differs:"; grep -v "^  ok" "$W/out.txt" | head -20; ERR=1
        fi
    else
        cmp -s "$W/$name/scenes.txt" "$W/release-fast/scenes.txt" || { echo "  FAIL  $name: scenes.txt differs from release-fast"; ERR=1; }
        for f in "$W/release-fast"/*.rgba; do
            cmp -s "$f" "$W/$name/$(basename "$f")" || { echo "  FAIL  $name: $(basename "$f") differs from release-fast"; ERR=1; }
        done
    fi
done
# counter-checks: damaged blend (+1) and a damaged font row (letter 'A' loses a pixel) must be detected
counter() { # <label> <sed script> <file>
    rm -rf "$W/badlib" "$W/bad"; mkdir -p "$W/badlib" "$W/bad" && cp -r lib/. "$W/badlib/"
    sed -i "$2" "$W/badlib/$3"
    if cmp -s "lib/$3" "$W/badlib/$3"; then echo "  FAIL  counter-check $1: the damage did not apply"; ERR=1; return; fi
    if FIRNLIB="$W/badlib" $FIRNC -o "$W/dump.bad" tools/mc/hud/dump.fi 2>/dev/null && "$W/dump.bad" "$W/bad" 2>/dev/null; then
        if python3 -I tools/mc/hud/ref.py "$W/bad" > "$W/bad.txt" 2>&1; then
            echo "  FAIL  counter-check $1: the damage passed"; ERR=1
        else
            echo "  ok    counter-check: $1 is detected ($(grep -c DIFF "$W/bad.txt") differences)"
        fi
    else
        echo "  FAIL  counter-check $1: build"; ERR=1
    fi
}
counter "damaged blend" 's|(sr \* a + (d \& 255 as u32) \* ia) / 255 as u32|(sr * a + (d \& 255 as u32) * ia + 1 as u32) / 255 as u32|' mc/mc_hud.fi
counter "damaged hotbar frame" 's|let ys: i32 = y0 - s$|let ys: i32 = y0 - s - 1|' mc/mc_hud.fi
counter "damaged font" 's|\\x0e\\x11\\x11\\x1f\\x11\\x11\\x11|\\x0e\\x11\\x11\\x1e\\x11\\x11\\x11|' mc/mc_hud_font.fi
if $FIRNC -o "$W/bench" tools/mc/hud/bench.fi 2>"$W/err"; then
    "$W/bench" | sed 's/^/  /'
else
    echo "  FAIL  bench.fi does not compile"; head -5 "$W/err"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-hud: ok" || echo "RESULT mc-hud: FAIL"
exit $ERR
