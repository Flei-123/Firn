#!/usr/bin/env bash
# tools/mc/perf/run.sh [--offline|--live] -- benchmark harness of the Minecraft client (docs/MC-PERF.md).
#   (no argument)  gate: statistics self test against numpy + smoke run (radius 2, 12 frames). Prints `RESULT mc-perf: ok`.
#   --offline      fixture world (tools/mc/view/fixture/world.mcw) tiled to the radius; CPU back end; radii PERF_RADII (8 16 32),
#                  PERF_RUNS (3) runs each; reports/perf_<stamp>_cpu_r<R>.tsv/.json. Start it via heavy (server is shared).
#   --live         first joins a local vanilla 1.20.1 server with demos/mcview (tools/mc/view/run.sh --live) -- NOT wired yet.
#   PERF_COMP=1      also run the mesh/upload component probe (perf_components): CRASHES (segfault) intermittently with R >= 8 on main 14.10. -> off by default
#   PERF_BACKEND=gl  (with --offline) uses tools/mc/perf/bench_gl.fi: GL back end (lib/gpu/gl_mc) on EGL surfaceless = llvmpipe, i.e. SOFTWARE GL, NO real GPU.
# Honesty: llvmpipe numbers are marked "no real GPU"; Vanilla is NOT measured (needs the Boss's ok).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
MODE=${1:-gate}; ERR=0
BENCH_SRC=tools/mc/perf/bench.fi; BTAG=cpu
[ "${PERF_BACKEND:-cpu}" = gl ] && { BENCH_SRC=tools/mc/perf/bench_gl.fi; BTAG=gl; unset DISPLAY EGL_PLATFORM; }
$FIRNC -o "$W/bench" "$BENCH_SRC" 2>"$W/err" || { echo "  FAIL  bench.fi does not compile"; head -5 "$W/err"; echo "RESULT mc-perf: FAIL"; exit 1; }
[ "$BTAG" = cpu ] && { python3 -I tools/mc/perf/stats_check.py "$W/bench" || ERR=1; }
if [ "$MODE" = gate ]; then
    BENCH_T0_NS=$(date +%s%N) "$W/bench" tools/mc/view/fixture/world.mcw 2 12 2 > "$W/smoke.txt" 2>&1
    if grep -q "RESULT frame_ms_p99" "$W/smoke.txt" && grep -q "RESULT rss_loaded_mb" "$W/smoke.txt"; then echo "  ok    smoke run (radius 2)"; else echo "  FAIL  smoke run"; tail -3 "$W/smoke.txt"; ERR=1; fi
else
    STAMP=$(date +%Y%m%d-%H%M%S); OUT=${PERF_OUT:-/root/jarvis/data/projects_kb/minecraft/reports}
    for R in ${PERF_RADII:-8 16 32}; do
        fs=()
        for i in $(seq 1 "${PERF_RUNS:-3}"); do
            LOAD=$(cut -d' ' -f1 /proc/loadavg)
            BENCH_COMP=${PERF_COMP:-0} BENCH_COMPUPLOAD=${PERF_COMP:-0} BENCH_T0_NS=$(date +%s%N) "$W/bench" tools/mc/view/fixture/world.mcw "$R" "${PERF_FRAMES:-100}" 10 > "$W/r${R}_$i.txt" 2>&1
            echo "RESULT server_load $LOAD load" >> "$W/r${R}_$i.txt"; fs+=("$W/r${R}_$i.txt")
        done
        if [ "$BTAG" = gl ]; then BN=gl-llvmpipe; GN="none (llvmpipe software GL, NO real GPU)"; else BN=cpu-raster; GN="none (CPU rasterizer, single thread)"; fi
        echo "{\"backend\":\"$BN\",\"gpu\":\"$GN\",\"radius\":$R,\"host\":\"$(hostname)\",\"cores\":$(nproc),\"world\":\"fixture tiled\",\"stamp\":\"$STAMP\"}" > "$W/meta.json"
        python3 -I tools/mc/perf/agg.py "$OUT/perf_${STAMP}_${BTAG}_r$R" "$W/meta.json" "${fs[@]}"
    done
fi
[ $ERR -eq 0 ] && echo "RESULT mc-perf: ok" || echo "RESULT mc-perf: FAIL"; exit $ERR
