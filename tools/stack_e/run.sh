#!/usr/bin/env bash
# tools/stack_e/run.sh -- gate 134: Firn shared object with C ABI (graphics stack way E, step 1).
# firnc --shared (one command) -> dlopen from C and ctypes from Python; --header=<file> -> C header check;
# G1: the Firn runtime (threads, GC, jobs) works INSIDE the .so (thread block behind gs, never the host's fs).
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
export FIRNLIB="$(pwd)/lib"
rc=0
for lvl in dev-fast release-safe release-fast; do
    "$FIRNC" --shared --header="$D/lib_e.h" --opt-level=$lvl -o "$D/libfirne.so" tools/stack_e/lib_e.fi || { echo "FAIL --shared $lvl"; rc=1; continue; }
    [ -e "$D/libfirne.o" ] || [ -e "$D/libfirne.ver" ] && { echo "FAIL $lvl: --shared left temp files"; rc=1; }
    nm -D --defined-only "$D/libfirne.so" | grep -qv ' se_' && { echo "FAIL $lvl: non-se_ symbol exported"; rc=1; }
    gcc -std=c11 -Wall -Werror -fsyntax-only -I"$D" tools/stack_e/hdr_check.c || { echo "FAIL $lvl: generated header does not compile"; rc=1; }
    readelf -d "$D/libfirne.so" | grep -q TEXTREL && { echo "FAIL $lvl: TEXTREL"; rc=1; }
    gcc -O1 -o "$D/host" tools/stack_e/host.c -ldl -lm || { echo "FAIL gcc"; rc=1; continue; }
    echo "== $lvl"
    "$D/host" "$D/libfirne.so" || rc=1
    python3 -I tools/stack_e/host.py "$D/libfirne.so" || rc=1
done
# runtime inside a .so: heap, jobs with and without workers, GC on several threads -- from C (with host threads of its own) and ctypes
printf '{ global: se_*; local: *; };\n' > "$D/exp.ver"
probe() { # $1 = call expression
    python3 -I -c "
import ctypes as C
l=C.CDLL('$D/librt.so'); l.se_heap_probe.restype=C.c_uint64; l.se_jobs_probe.restype=C.c_uint64; l.se_gc_probe.restype=C.c_uint64
buf=(C.c_uint32*8192)(); r=$1; print(r, buf[100], buf[4095])" 2>&1
}
for lvl in dev-fast release-safe release-fast; do
    "$FIRNC" --shared --opt-level=$lvl -o "$D/librt.so" tools/stack_e/lib_rt.fi || { echo "FAIL lib_rt --shared $lvl"; rc=1; continue; }
    objdump -d "$D/librt.so" | grep -q '%fs:' && { echo "FAIL $lvl: .so still reads the host's fs"; rc=1; }
    [ "$(probe 'l.se_heap_probe(1000)')" = "499500 0 0" ] && echo "ok  heap_alloc in .so ($lvl)" || { echo "FAIL heap_alloc in .so"; rc=1; }
    [ "$(probe 'l.se_jobs_probe(buf,4096,0)')" = "4096 300 12285" ] && echo "ok  jobs.parallel_for, 0 workers, in .so ($lvl)" || { echo "FAIL jobs 0 workers in .so"; rc=1; }
    if [ "$(probe 'l.se_jobs_probe(buf,4096,3)')" = "4096 300 12285" ]; then echo "GAP CLOSED: jobs with 3 workers == 0 workers in a .so, ctypes ($lvl)"
    else echo "FAIL G1: jobs with workers in a .so breaks the host ($lvl)"; rc=1; fi
    [ "$(probe 'l.se_gc_probe(3)')" = "1600 0 0" ] && echo "ok  GC on 4 threads in .so, ctypes ($lvl)" || { echo "FAIL GC probe in .so (ctypes)"; rc=1; }
    gcc -O1 -fstack-protector-all -pthread -o "$D/hostrt" tools/stack_e/host_rt.c -ldl || { echo "FAIL gcc host_rt"; rc=1; continue; }
    timeout 120 "$D/hostrt" "$D/librt.so" | tail -1; [ "${PIPESTATUS[0]}" = 0 ] || { echo "FAIL host_rt ($lvl)"; rc=1; }
    # G1b: several host threads call into the .so (thread_attach / thread_host_detach), C and ctypes
    gcc -O1 -fstack-protector-all -pthread -o "$D/hostatt" tools/stack_e/host_attach.c -ldl || { echo "FAIL gcc host_attach"; rc=1; continue; }
    timeout 120 "$D/hostatt" "$D/librt.so" | tail -1; [ "${PIPESTATUS[0]}" = 0 ] || { echo "FAIL host_attach C ($lvl)"; rc=1; }
    timeout 120 python3 -I tools/stack_e/host_attach.py "$D/librt.so" || { echo "FAIL host_attach ctypes ($lvl)"; rc=1; }
done
# the new v128 intrinsics, all stages
for st in "--opt-level=dev-fast" "--opt-level=release-safe" "--opt-level=release-fast" "--opt-level=release-fast --asm-intern"; do
    "$FIRNC" $st -o "$D/t2860" tests/2860_simd_minmax.fi 2> "$D/err" && "$D/t2860" && echo "ok  tests/2860 $st" || { echo "FAIL tests/2860 $st"; head -3 "$D/err"; rc=1; }
done
# G6: sqrt/rcp/rsqrt, shufps, movemask, packs/unpacks, min/max, cmpeq, mulhi -- scalar reference tests (x86 all stages, NEON under qemu), numpy cross-check
for t in tests/2867_simd_g6.fi tests/2868_std_simd.fi; do
    for st in "--opt-level=dev-fast" "--opt-level=release-safe" "--opt-level=release-fast" "--opt-level=release-fast --asm-intern"; do
        "$FIRNC" $st -o "$D/tg6" $t 2> "$D/err" && "$D/tg6" && echo "ok  $t $st" || { echo "FAIL $t $st"; head -3 "$D/err"; rc=1; }
    done
    if command -v qemu-aarch64 >/dev/null; then
        for st in "--opt-level=dev-fast" "--opt-level=release-fast"; do
            "$FIRNC" --target=aarch64-linux $st -o "$D/tg6a" $t 2> "$D/err" && qemu-aarch64 "$D/tg6a" && echo "ok  $t aarch64/qemu $st" || { echo "FAIL $t aarch64 $st"; head -3 "$D/err"; rc=1; }
        done
    fi
done
for lvl in dev-fast release-fast; do
    "$FIRNC" --shared --opt-level=$lvl -o "$D/libsimd.so" tools/stack_e/lib_simd.fi && python3 -I tools/stack_e/host_simd.py "$D/libsimd.so" || { echo "FAIL host_simd ($lvl)"; rc=1; }
done
if "$FIRNC" --opt-level=release-fast -o "$D/sb" tools/stack_e/simd_bench.fi 2> "$D/err"; then "$D/sb" | sed 's/^/  /'; else echo "FAIL simd_bench"; rc=1; fi
[ $rc = 0 ] && echo "RESULT stack_e: ok" || echo "RESULT stack_e: FAIL"
exit $rc
