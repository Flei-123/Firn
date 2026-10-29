#!/usr/bin/env bash
# tools/toctou/check.sh -- gate 140: no compiler-invented loads on shared memory.
# Compiles tools/toctou/toctou.fi for x86-64, aarch64 and wasm32 at every build
# level and checks (check.py) that each snapshot function reads the shared
# symbol exactly once. A mutation run proves the checker can fail.
set -euo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-compiler/target/release/firnc}"
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
rc=0
for lvl in dev dev-fast release-safe release-fast; do
    "$FIRNC" --emit=asm --opt-level=$lvl tools/toctou/toctou.fi -o "$W/x.s" > "$W/c.log" 2>&1 || { cat "$W/c.log"; exit 1; }
    "$FIRNC" --target=aarch64-linux --emit=asm --opt-level=$lvl tools/toctou/toctou.fi -o "$W/a.s" > "$W/c.log" 2>&1 || { cat "$W/c.log"; exit 1; }
    for k in "x86 $W/x.s" "a64 $W/a.s"; do
        set -- $k
        if python3 -I tools/toctou/check.py $1 $2 > "$W/r.log" 2>&1; then echo "  ok  $1 $lvl: $(tail -1 $W/r.log)"; else echo "  FAIL $1 $lvl"; cat "$W/r.log"; rc=1; fi
    done
    if command -v wasm2wat >/dev/null; then
        "$FIRNC" --target=wasm32-browser --opt-level=$lvl tools/toctou/toctou.fi -o "$W/w.wasm" > "$W/c.log" 2>&1 || { cat "$W/c.log"; exit 1; }
        wasm2wat "$W/w.wasm" > "$W/w.wat"
        if python3 -I tools/toctou/check.py wasm "$W/w.wat" > "$W/r.log" 2>&1; then echo "  ok  wasm $lvl: $(tail -1 $W/r.log)"; else echo "  FAIL wasm $lvl"; cat "$W/r.log"; rc=1; fi
    else echo "  skip wasm $lvl (no wasm2wat)"; fi
done
# the checker must be able to fail
if python3 -I tools/toctou/check.py x86 "$W/x.s" --mutate > /dev/null 2>&1; then echo "  FAIL mutation not detected"; rc=1; else echo "  ok  mutation detected"; fi
if command -v wasm2wat >/dev/null; then
    "$FIRNC" --target=wasm32-browser --opt-level=release-fast tools/toctou/toctou.fi -o "$W/w.wasm" > "$W/c.log" 2>&1 && wasm2wat "$W/w.wasm" > "$W/w.wat"
    if python3 -I tools/toctou/check.py wasm "$W/w.wat" --mutate > /dev/null 2>&1; then echo "  FAIL wasm mutation not detected"; rc=1; else echo "  ok  wasm mutation detected"; fi
fi
exit $rc
