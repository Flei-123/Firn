#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/stack_e/run_g5.sh -- G5: `firnc --shared` for the Windows DLL (x86_64-windows, run under Wine) and the aarch64 .so (run under qemu-aarch64).
# Called by run.sh (gate 134); also usable alone. Wine stages share ~/.wine-firn: never run two at once.
# NOT covered: a real Windows, a real aarch64 machine, floats / v128 in DLL exports (refused), Firn threads in an aarch64 .so (refused),
# host threads in a DLL (thread_attach is only proven for the Linux x86-64 .so).
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
export FIRNLIB="$(pwd)/lib"
rc=0
# G5: Windows DLL (x86_64-windows, Wine) and aarch64 .so (qemu-aarch64). Integer / pointer exports only.
# The Wine tiers share ~/.wine-firn: never run two Wine stages at once.
if command -v wine >/dev/null && command -v x86_64-w64-mingw32-gcc >/dev/null; then
    x86_64-w64-mingw32-gcc -O1 -o "$D/host_win.exe" tools/stack_e/host_win.c || { echo "FAIL mingw host_win"; rc=1; }
    for lvl in dev-fast release-fast; do
        "$FIRNC" --shared --target=x86_64-windows --opt-level=$lvl -o "$D/wd.dll" tools/stack_e/lib_win.fi || { echo "FAIL dll $lvl"; rc=1; continue; }
        [ -e "$D/wd.def" ] || [ -e "$D/wd.o" ] && { echo "FAIL dll $lvl: temp files left"; rc=1; }
        x86_64-w64-mingw32-objdump -p "$D/wd.dll" | grep -q 'wd_gc_chains' || { echo "FAIL dll $lvl: export missing"; rc=1; }
        (cd "$D" && WINEPREFIX=${WINEPREFIX:-$HOME/.wine-firn} WINEDEBUG=-all timeout 120 wine host_win.exe wd.dll win 2>&1 | grep -q 'all ok') && echo "ok  Windows DLL under Wine ($lvl)" || { echo "FAIL DLL host (wine, $lvl)"; rc=1; }
    done
    "$FIRNC" --shared --target=x86_64-windows -o "$D/rt.dll" tools/stack_e/lib_rt.fi || { echo "FAIL dll lib_rt"; rc=1; }
    (cd "$D" && WINEPREFIX=${WINEPREFIX:-$HOME/.wine-firn} WINEDEBUG=-all timeout 120 wine host_win.exe rt.dll rt 2>&1 | grep -q 'all ok') && echo "ok  runtime (heap, jobs, GC on 4 threads) in a Windows DLL" || { echo "FAIL DLL runtime (wine)"; rc=1; }
    # floats are refused with a clear error, not miscompiled
    printf '#[export_c]\nfn bad_f(x: f64) -> f64 { return x }\n' > "$D/badf.fi"
    "$FIRNC" --shared --target=x86_64-windows -o "$D/badf.dll" "$D/badf.fi" 2> "$D/err" && { echo "FAIL dll: float export accepted"; rc=1; } || grep -q 'float' "$D/err" || { echo "FAIL dll: float error text"; rc=1; }
else echo "skip  Windows DLL (no wine / mingw)"; fi
if command -v qemu-aarch64 >/dev/null && command -v aarch64-linux-gnu-gcc >/dev/null && [ -d /usr/aarch64-linux-gnu/lib ]; then
    aarch64-linux-gnu-gcc -O1 -fstack-protector-all -o "$D/host_a64" tools/stack_e/host_a64.c -ldl || { echo "FAIL aarch64 gcc host"; rc=1; }
    for lvl in dev-fast release-fast; do
        "$FIRNC" --shared --target=aarch64-linux --opt-level=$lvl -o "$D/a64.so" tools/stack_e/lib_win.fi || { echo "FAIL aarch64 --shared $lvl"; rc=1; continue; }
        readelf -d "$D/a64.so" | grep -q TEXTREL && { echo "FAIL aarch64 $lvl: TEXTREL"; rc=1; }
        (cd "$D" && timeout 300 qemu-aarch64 -L /usr/aarch64-linux-gnu ./host_a64 ./a64.so 2>&1 | grep -q 'all ok') && echo "ok  aarch64 .so under qemu ($lvl)" || { echo "FAIL aarch64 .so host ($lvl)"; rc=1; }
    done
else echo "skip  aarch64 .so (no qemu-aarch64 / cross gcc / sysroot)"; fi
[ $rc = 0 ] && echo "RESULT g5: ok" || echo "RESULT g5: FAIL"
exit $rc
