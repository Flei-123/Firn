#!/usr/bin/env bash
# tools/mc/perf/build_win.sh <outdir> -- cross build of the benchmark harness for Windows (bench.exe) + wine smoke test.
# Run on FLEI-ONE:  bench.exe world.mcw <radius> <frames> <warmup>   -> RESULT lines on stdout (same as the Linux harness).
set -euo pipefail
cd "$(dirname "$0")/../../.."
OUT=${1:?outdir}; mkdir -p "$OUT"
FIRNC=${FIRNC:-compiler/target/release/firnc}; export FIRNLIB="$(pwd)/lib"
$FIRNC --target=x86_64-windows -o "$OUT/bench.exe" tools/mc/perf/bench.fi
cp tools/mc/view/fixture/world.mcw "$OUT/"
if command -v wine >/dev/null; then
    (cd "$OUT" && WINEDEBUG=-all WINEPREFIX="$OUT/.wp" timeout 150 wine bench.exe world.mcw 2 5 2 | grep -c "RESULT frame_ms_p50") | sed 's/^/wine smoke RESULT lines: /'
    rm -rf "$OUT/.wp"
fi
