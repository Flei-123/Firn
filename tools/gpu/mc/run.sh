#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/gpu/mc/run.sh -- gate 132: scene layer (lib/gpu/gl_scene.fi) against the CPU path, PSNR >= 34 (docs/GPU.md "Scene layer").
#   scene.fi headless (sky, world+sky+fog+tint+HUD, HUD only, entities, entities+fog, cull on/off byte equal, MSAA) and once per
#   window API (EGL, GLX) under Xvfb with the window-present path; upload_probe.fi prints raw upload timings (info only).
# SKIP (exit 0) without libEGL. Usage: bash tools/gpu/mc/run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
[ -e /usr/lib/x86_64-linux-gnu/libEGL.so.1 ] || { echo "   SKIP: no libEGL.so.1"; exit 0; }
D=$(mktemp -d); XP=""
trap '[ -n "$XP" ] && kill "$XP" 2>/dev/null; rm -rf "$D"' EXIT
"$FIRNC" -o "$D/scene" tools/gpu/mc/scene.fi 2>"$D/err" || { echo "   FAIL: compile scene.fi"; head -5 "$D/err"; exit 1; }
"$FIRNC" -o "$D/eb" tools/gpu/mc/ent_bench.fi 2>"$D/err" || { echo "   FAIL: compile ent_bench.fi"; head -5 "$D/err"; exit 1; }
"$FIRNC" -o "$D/ly" tools/gpu/mc/layers.fi 2>"$D/err" || { echo "   FAIL: compile layers.fi"; head -5 "$D/err"; exit 1; }
"$FIRNC" -o "$D/up" tools/gpu/mc/upload_probe.fi 2>"$D/err" || { echo "   FAIL: compile upload_probe.fi"; head -5 "$D/err"; exit 1; }
fail=0
env -u DISPLAY -u EGL_PLATFORM timeout 180 "$D/scene" >"$D/h.txt" 2>&1; rc=$?
sed 's/^/   headless: /' "$D/h.txt"
if [ "$rc" -eq 0 ] && grep -q "^scene layer: all ok" "$D/h.txt" && [ "$(grep -c '^scene .* ok$' "$D/h.txt")" -ge 8 ]; then
    echo "   ok:   scene layer vs mc_raster headless"
else
    echo "   FAIL: scene layer headless (exit $rc)"; fail=1
fi
env -u DISPLAY -u EGL_PLATFORM timeout 120 "$D/up" 2>&1 | sed 's/^/   upload: /'
env -u DISPLAY -u EGL_PLATFORM timeout 200 "$D/eb" 2>&1 | grep -E "^(renderer|metric ents|ents summary)" | sed 's/^/   ents: /'
env -u DISPLAY -u EGL_PLATFORM timeout 200 "$D/ly" >"$D/ly.txt" 2>&1; rc=$?
grep -E "^(metric|check|layers)" "$D/ly.txt" | sed 's/^/   layers: /'
if [ "$rc" -eq 0 ] && grep -q "^layers: all ok" "$D/ly.txt"; then echo "   ok:   texture array 1120 layers / layer limit / unequal-section defrag / mip+aniso"; else echo "   FAIL: layers.fi (exit $rc)"; fail=1; fi
if command -v Xvfb >/dev/null 2>&1; then
    mkfifo "$D/disp"; Xvfb -displayfd 3 -screen 0 800x600x24 -nolisten tcp 3>"$D/disp" >/dev/null 2>&1 & XP=$!
    read -r -t 20 n <"$D/disp" || n=""
    if [ -z "$n" ]; then
        echo "   SKIP: window path (Xvfb did not start)"
    else
        for m in egl glx; do
            DISPLAY=":$n" timeout 180 "$D/scene" $m >"$D/w$m.txt" 2>&1; rc=$?
            grep -E "window-vs-cpu|FAIL" "$D/w$m.txt" | sed "s/^/   $m: /"
            if [ "$rc" -eq 0 ] && grep -q "^scene window-vs-cpu .* ok$" "$D/w$m.txt"; then
                echo "   ok:   scene layer through the $m window (present without read back)"
            else
                echo "   FAIL: scene layer through the $m window (exit $rc)"; fail=1
            fi
        done
    fi
else
    echo "   SKIP: window path (no Xvfb)"
fi
exit $fail
