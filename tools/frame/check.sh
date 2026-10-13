#!/usr/bin/env bash
# tools/frame/check.sh -- x86-64 frame layout gate (r477): the save slots of the
# callee-saved registers are 8-aligned to rbp in EVERY function (odd byte locals
# used to push them to rbp-0x49 and hide Gc pointers from the conservative scan,
# r476). Checks the --emit=asm listing of tools/frame/odd_frames.fi (+ the
# collector code linked in) and of tests/2872 and lib/layout/b2_main.fi (the
# layoutb2 engine). Then runs the deterministic layoutb2 GC reproducer.
set -euo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-compiler/target/release/firnc}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
rc=0
for src in tools/frame/odd_frames.fi tests/2872_gc_unaligned_save_slots.fi lib/layout/b2_main.fi; do
    "$FIRNC" --emit=asm -o "$W/a.s" "$src" > "$W/c.log" 2>&1 || { echo "  compile failed: $src"; tail -5 "$W/c.log"; exit 1; }
    req=()
    [ "$src" = tools/frame/odd_frames.fi ] && req=(--require-odd f1 f3 f5 f7)
    if python3 -I tools/frame/check.py "$W/a.s" "${req[@]}" > "$W/r.log" 2>&1; then
        echo "  ok  $src: $(head -1 "$W/r.log")"
    else
        echo "  FAIL $src"; cat "$W/r.log"; rc=1
    fi
done
[ "$rc" -eq 0 ] || exit "$rc"
if [ "${FRAME_SKIP_REPRO:-0}" != 1 ]; then
    bash tools/layoutb2/gc_repro.sh > "$W/repro.log" 2>&1 && echo "  ok  layoutb2 gc_repro: $(tail -1 "$W/repro.log")" \
        || { echo "  FAIL layoutb2 gc_repro"; tail -6 "$W/repro.log"; exit 1; }
fi
