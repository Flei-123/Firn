#!/usr/bin/env bash
# tools/mc/perf/build_win.sh <outdir> -- cross build of the benchmark harness for Windows (bench.exe) + wine smoke test.
# Run on FLEI-ONE:  bench.exe (CPU) / bench_gl.exe (OpenGL) / bench_vk.exe (Vulkan, vulkan-1.dll) world.mcw <radius> <frames> <warmup>   -> RESULT lines on stdout (same as the Linux harness).
set -euo pipefail
cd "$(dirname "$0")/../../.."
OUT=${1:?outdir}; mkdir -p "$OUT"
FIRNC=${FIRNC:-compiler/target/release/firnc}; export FIRNLIB="$(pwd)/lib"
for t in bench bench_gl bench_vk; do $FIRNC --target=x86_64-windows -o "$OUT/$t.exe" tools/mc/perf/$t.fi; done
cp tools/mc/view/fixture/world.mcw "$OUT/"
if command -v wine >/dev/null; then
    (cd "$OUT" && WINEDEBUG=-all WINEPREFIX="$OUT/.wp" timeout 150 wine bench.exe world.mcw 2 5 2 | grep -c "RESULT frame_ms_p50") | sed 's/^/wine smoke RESULT lines: /'
    rm -rf "$OUT/.wp"
fi
