#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/wasm/gc_soak.sh -- THE COLLECTOR UNDER WEBASSEMBLY (round WASM, step 3).
#
# WebAssembly cannot scan its own stack, so codegen_wasm.rs puts every
# pointer sized value that is live across a collecting call into the
# caller's frame on the SHADOW stack, and the collector (lib/gc/gc.fi,
# unchanged) scans that. This script proves it, three times over:
#
#   1. tools/wasm/gc_soak.fi natively -- the reference verdict
#   2. the same source as WebAssembly under node, in three build levels:
#      intact after at least 1000 collections, and the SAME line as native
#   3. THE COUNTER-CHECK: the same source with FIRN_WASM_NO_SPILL=1 (the
#      spills switched off, nothing else) MUST report corruption. If it
#      passed, the soak test would not be able to see a missing root, and
#      step 2 would prove nothing.
#
# Usage: bash tools/wasm/gc_soak.sh      (exit 0 = all three hold)
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC=${FIRNC:-compiler/target/release/firnc}
W=${W:-/tmp/firn-wasm-gc-soak}
mkdir -p "$W"
fail=0

echo "== 1. native reference =="
"$FIRNC" -o "$W/soak" tools/wasm/gc_soak.fi 2>&1 | grep -v 'RWX permissions' || true
"$W/soak" > "$W/native.out"; nrc=$?
cat "$W/native.out"
if [ "$nrc" != 0 ]; then
    echo "   native reference FAILED (exit $nrc)"
    fail=1
fi

echo "== 2. WebAssembly under node, three build levels =="
for lvl in dev dev-fast release-fast; do
    "$FIRNC" --opt-level=$lvl --target=wasm32-browser -o "$W/soak_$lvl.wasm" tools/wasm/gc_soak.fi || { fail=1; continue; }
    t0=$(date +%s%N)
    node tools/wasm/run.mjs "$W/soak_$lvl.wasm" > "$W/wasm_$lvl.out"; wrc=$?
    t1=$(date +%s%N)
    ms=$(( (t1 - t0) / 1000000 ))
    # The verdict line has to be the native one; the collection count
    # depends on the speed of the machine (time budgeted slices), so it is
    # shown and has a floor (the program exits 2 below 1000), but it is
    # not compared.
    if [ "$wrc" = 0 ] && [ "$(head -1 "$W/native.out")" = "$(head -1 "$W/wasm_$lvl.out")" ]; then
        printf "   %-12s exit 0, verdict as native, %s (%d ms)   OK\n" "$lvl" "$(sed -n 2p "$W/wasm_$lvl.out" | sed 's/gc_soak: //')" "$ms"
    else
        printf "   %-12s exit %s: %s   FAILED\n" "$lvl" "$wrc" "$(tr '\n' ' ' < "$W/wasm_$lvl.out")"
        fail=1
    fi
done

echo "== 3. counter-check: the spills switched off (FIRN_WASM_NO_SPILL=1) =="
FIRN_WASM_NO_SPILL=1 "$FIRNC" --target=wasm32-browser -o "$W/soak_nospill.wasm" tools/wasm/gc_soak.fi || fail=1
node tools/wasm/run.mjs "$W/soak_nospill.wasm" > "$W/nospill.out" 2>&1; crc=$?
echo "   $(head -1 "$W/nospill.out") (exit $crc)"
if [ "$crc" = 0 ]; then
    echo "   the soak test PASSED without the spills -- it cannot see a missing root   FAILED"
    fail=1
else
    echo "   without the spills the collector frees live objects, and the test sees it   OK"
fi

if [ "$fail" = 0 ]; then
    echo "GC SOAK PASSED"
else
    echo "GC SOAK FAILED"
fi
exit $fail
