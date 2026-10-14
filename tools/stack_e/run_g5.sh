#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/stack_e/run_g5.sh -- G5: `firnc --shared` for the Windows DLL (x86_64-windows, run under Wine) and the aarch64 .so (run under qemu-aarch64).
# Called by run.sh (gate 134); also usable alone. Wine stages share ~/.wine-firn: never run two at once.
# NOT covered: a real Windows, a real aarch64 machine, by-value structs in exports, v128 beyond xmm7 in DLL exports (refused).
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
export FIRNLIB="$(pwd)/lib"
rc=0
# G5: Windows DLL (x86_64-windows, Wine) and aarch64 .so (qemu-aarch64): integer, pointer, f32/f64 exports, more arguments than registers.
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
    # floats and stack arguments: Win64 thunk (rcx/rdx/r8/r9 | xmm0-3 | stack  ->  System V placement), same C host as the .so tiers
    x86_64-w64-mingw32-gcc -O1 -o "$D/host_abi.exe" tools/stack_e/host_abi.c || { echo "FAIL mingw host_abi"; rc=1; }
    for lvl in dev-fast release-fast; do
        "$FIRNC" --shared --target=x86_64-windows --opt-level=$lvl -o "$D/abi.dll" tools/stack_e/lib_abi.fi || { echo "FAIL dll lib_abi $lvl"; rc=1; continue; }
        (cd "$D" && WINEPREFIX=${WINEPREFIX:-$HOME/.wine-firn} WINEDEBUG=-all timeout 120 wine host_abi.exe abi.dll 2>&1 | grep -q 'host_abi: all ok') && echo "ok  Windows DLL floats + stack arguments under Wine ($lvl)" || { echo "FAIL DLL float/stack ABI (wine, $lvl)"; rc=1; }
    done
    # host threads of their own in a DLL (r506): 4 CreateThread threads attach/detach in two waves (entries recycled), jobs + GC
    x86_64-w64-mingw32-gcc -O1 -o "$D/hat.exe" tools/stack_e/host_attach_win.c || { echo "FAIL mingw host_attach_win"; rc=1; }
    for lvl in dev-fast release-fast; do
        "$FIRNC" --shared --target=x86_64-windows --opt-level=$lvl -o "$D/rta.dll" tools/stack_e/lib_rt.fi || { echo "FAIL dll lib_rt (attach) $lvl"; rc=1; continue; }
        ok=1
        for rep in 1 2 3; do
            (cd "$D" && WINEPREFIX=${WINEPREFIX:-$HOME/.wine-firn} WINEDEBUG=-all timeout 300 wine hat.exe rta.dll 2>&1 | tail -1 | grep -q 'ok  host_attach') || { echo "FAIL DLL attached host threads (wine, $lvl, run $rep)"; rc=1; ok=0; break; }
        done
        [ $ok = 1 ] && echo "ok  Windows DLL attached host threads under Wine ($lvl)"
    done
    # a v128 parameter that finds no xmm0-7 register is refused with a clear error (the others are passed: abi_v128_* above)
    printf '#[export_c]\nfn bad_v(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64, g: f64, h: f64, x: v128) -> u64 { return 0 }\n' > "$D/badv.fi"
    "$FIRNC" --shared --target=x86_64-windows -o "$D/badv.dll" "$D/badv.fi" 2> "$D/err" && { echo "FAIL dll: v128 export accepted"; rc=1; } || grep -q 'v128' "$D/err" || { echo "FAIL dll: v128 error text"; rc=1; }
else echo "skip  Windows DLL (no wine / mingw)"; fi
if command -v qemu-aarch64 >/dev/null && command -v aarch64-linux-gnu-gcc >/dev/null && [ -d /usr/aarch64-linux-gnu/lib ]; then
    aarch64-linux-gnu-gcc -O1 -fstack-protector-all -o "$D/host_a64" tools/stack_e/host_a64.c -ldl || { echo "FAIL aarch64 gcc host"; rc=1; }
    aarch64-linux-gnu-gcc -O1 -o "$D/host_abi_a" tools/stack_e/host_abi.c -ldl || { echo "FAIL aarch64 gcc host_abi"; rc=1; }
    aarch64-linux-gnu-gcc -O1 -fstack-protector-all -pthread -o "$D/hostrt_a" tools/stack_e/host_rt.c -ldl || { echo "FAIL aarch64 gcc host_rt"; rc=1; }
    aarch64-linux-gnu-gcc -O1 -fstack-protector-all -pthread -o "$D/hostatt_a" tools/stack_e/host_attach.c -ldl || { echo "FAIL aarch64 gcc host_attach"; rc=1; }
    for lvl in dev-fast release-fast; do
        "$FIRNC" --shared --target=aarch64-linux --opt-level=$lvl -o "$D/abi_a.so" tools/stack_e/lib_abi.fi || { echo "FAIL aarch64 lib_abi $lvl"; rc=1; }
        (cd "$D" && timeout 120 qemu-aarch64 -L /usr/aarch64-linux-gnu ./host_abi_a ./abi_a.so 2>&1 | grep -q 'host_abi: all ok') && echo "ok  aarch64 .so floats + stack arguments under qemu ($lvl)" || { echo "FAIL aarch64 float/stack ABI ($lvl)"; rc=1; }
        # Firn threads (thread_start, jobs workers, GC on 4 threads) and attached host threads in an aarch64 .so
        "$FIRNC" --shared --target=aarch64-linux --opt-level=$lvl -o "$D/rt_a.so" tools/stack_e/lib_rt.fi || { echo "FAIL aarch64 lib_rt $lvl"; rc=1; continue; }
        (cd "$D" && timeout 600 qemu-aarch64 -L /usr/aarch64-linux-gnu ./hostrt_a ./rt_a.so 2>&1 | tail -1 | grep -q 'ok') && echo "ok  aarch64 .so Firn threads + GC + jobs under qemu ($lvl)" || { echo "FAIL aarch64 .so threads (host_rt, $lvl)"; rc=1; }
        (cd "$D" && timeout 600 qemu-aarch64 -L /usr/aarch64-linux-gnu ./hostatt_a ./rt_a.so 2>&1 | tail -1 | grep -q 'ok') && echo "ok  aarch64 .so attached host threads under qemu ($lvl)" || { echo "FAIL aarch64 .so host_attach ($lvl)"; rc=1; }
        "$FIRNC" --shared --target=aarch64-linux --opt-level=$lvl -o "$D/a64.so" tools/stack_e/lib_win.fi || { echo "FAIL aarch64 --shared $lvl"; rc=1; continue; }
        readelf -d "$D/a64.so" | grep -q TEXTREL && { echo "FAIL aarch64 $lvl: TEXTREL"; rc=1; }
        (cd "$D" && timeout 300 qemu-aarch64 -L /usr/aarch64-linux-gnu ./host_a64 ./a64.so 2>&1 | grep -q 'all ok') && echo "ok  aarch64 .so under qemu ($lvl)" || { echo "FAIL aarch64 .so host ($lvl)"; rc=1; }
    done
else echo "skip  aarch64 .so (no qemu-aarch64 / cross gcc / sysroot)"; fi
[ $rc = 0 ] && echo "RESULT g5: ok" || echo "RESULT g5: FAIL"
exit $rc
