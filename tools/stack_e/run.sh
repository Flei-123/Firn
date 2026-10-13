#!/usr/bin/env bash
# tools/stack_e/run.sh -- gate 134: Firn shared object with C ABI (graphics stack way E, step 1).
# firnc --pic -c -> ld -shared -> dlopen from C and ctypes from Python.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
export FIRNLIB="$(pwd)/lib"
rc=0
for lvl in dev-fast release-safe release-fast; do
    "$FIRNC" --pic --opt-level=$lvl -c -o "$D/lib_e.o" tools/stack_e/lib_e.fi || { echo "FAIL compile $lvl"; rc=1; continue; }
    printf '{ global: se_*; local: *; };\n' > "$D/exp.ver"
    ld -shared -Bsymbolic --version-script="$D/exp.ver" -z noexecstack -soname libfirne.so -o "$D/libfirne.so" "$D/lib_e.o" || { echo "FAIL link $lvl"; rc=1; continue; }
    readelf -d "$D/libfirne.so" | grep -q TEXTREL && { echo "FAIL $lvl: TEXTREL"; rc=1; }
    gcc -O1 -o "$D/host" tools/stack_e/host.c -ldl -lm || { echo "FAIL gcc"; rc=1; continue; }
    echo "== $lvl"
    "$D/host" "$D/libfirne.so" || rc=1
    python3 -I tools/stack_e/host.py "$D/libfirne.so" || rc=1
done
# runtime inside a .so: heap + jobs without workers must work; jobs WITH workers is a KNOWN GAP (fs:0 TLS clash with glibc)
"$FIRNC" --pic --opt-level=release-safe -c -o "$D/lib_rt.o" tools/stack_e/lib_rt.fi \
  && ld -shared -Bsymbolic --version-script="$D/exp.ver" -z noexecstack -soname librt.so -o "$D/librt.so" "$D/lib_rt.o" || { echo "FAIL lib_rt build"; rc=1; }
probe() { # $1 = call expression
    python3 -I -c "
import ctypes as C
l=C.CDLL('$D/librt.so'); l.se_heap_probe.restype=C.c_uint64; l.se_jobs_probe.restype=C.c_uint64
buf=(C.c_uint32*8192)(); r=$1; print(r, buf[100], buf[4095])" 2>&1
}
[ "$(probe 'l.se_heap_probe(1000)')" = "499500 0 0" ] && echo "ok  heap_alloc in .so" || { echo "FAIL heap_alloc in .so"; rc=1; }
[ "$(probe 'l.se_jobs_probe(buf,4096,0)')" = "4096 300 12285" ] && echo "ok  jobs.parallel_for, 0 workers, in .so" || { echo "FAIL jobs 0 workers in .so"; rc=1; }
if [ "$(probe 'l.se_jobs_probe(buf,4096,3)')" = "4096 300 12285" ]; then echo "GAP CLOSED: jobs with workers works in a .so -- update docs/GRAPHICS-STACK-E-REQUIREMENTS.md"
else echo "known gap G1: jobs with workers in a .so corrupts the host (Firn TLS fs:0 vs glibc TCB)"; fi
# the new v128 intrinsics, all stages
for st in "--opt-level=dev-fast" "--opt-level=release-safe" "--opt-level=release-fast" "--opt-level=release-fast --asm-intern"; do
    "$FIRNC" $st -o "$D/t2860" tests/2860_simd_minmax.fi 2> "$D/err" && "$D/t2860" && echo "ok  tests/2860 $st" || { echo "FAIL tests/2860 $st"; head -3 "$D/err"; rc=1; }
done
if "$FIRNC" --opt-level=release-fast -o "$D/sb" tools/stack_e/simd_bench.fi 2> "$D/err"; then "$D/sb" | sed 's/^/  /'; else echo "FAIL simd_bench"; rc=1; fi
[ $rc = 0 ] && echo "RESULT stack_e: ok" || echo "RESULT stack_e: FAIL"
exit $rc
