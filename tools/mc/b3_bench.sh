#!/usr/bin/env bash
# tools/mc/b3_bench.sh -- B3 check: what does float register allocation (xmm-ra) buy the renderer loops?
#
# Counts EXECUTED INSTRUCTIONS with callgrind (reproducible, independent of host load) for
# tools/mc/raster/bench.fi and tools/mc/sky/bench.fi, per frame (difference of two runs with
# different frame counts, so the one-off setup cancels out), and the median wall time of the
# unmodified benches.
#
# Usage: bash tools/mc/b3_bench.sh <label>=<firnc>[:ENV=1] ...
#   e.g. bash tools/mc/b3_bench.sh main-noxmm=/path/firnc:FIRN_NO_FP_RA=1 main=/path/firnc
# Output: one line per label and metric:  RESULT <label> <metric> <value>
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT

python3 -I - "$W" <<'PY'
import re, sys
w = sys.argv[1]
src = open("tools/mc/raster/bench.fi").read()
cut = src.index("    // tiny triangles")
head = src[:cut] + "    return 0\n}\n"
for n in (2, 6):
    open(f"{w}/raster_{n}.fi", "w").write(head.replace("let frames: i32 = 20", f"let frames: i32 = {n}"))
sky = open("tools/mc/sky/bench.fi").read()
for n in (2, 6):
    open(f"{w}/sky_{n}.fi", "w").write(sky.replace("while i < 10 {", f"while i < {n} {{"))
PY

for spec in "$@"; do
    label=${spec%%=*}; rest=${spec#*=}; bin=${rest%%:*}; envs=""
    [ "$rest" != "$bin" ] && envs=${rest#*:}
    for b in raster sky; do
        for n in 2 6; do
            env $envs "$bin" ${FIRNC_FLAGS:-} -o "$W/$b.$n.$label" "$W/${b}_$n.fi" 2>"$W/err" || { echo "compile fail $label $b"; head -3 "$W/err"; continue 2; }
            valgrind --tool=callgrind --callgrind-out-file="$W/cg.$b.$n.$label" "$W/$b.$n.$label" >/dev/null 2>"$W/vg" || true
            eval "ir_${b}_$n=\$(grep -E '^summary:' $W/cg.$b.$n.$label | awk '{print \$2}')"
        done
    done
    echo "RESULT $label raster_Ir_per_frame $(( (ir_raster_6 - ir_raster_2) / 4 ))"
    echo "RESULT $label sky_Ir_per_iter_3scenes $(( (ir_sky_6 - ir_sky_2) / 4 ))"
    # wall time: original benches, median of 7
    env $envs "$bin" ${FIRNC_FLAGS:-} -o "$W/rb.$label" tools/mc/raster/bench.fi 2>/dev/null
    env $envs "$bin" ${FIRNC_FLAGS:-} -o "$W/sb.$label" tools/mc/sky/bench.fi 2>/dev/null
    for i in 1 2 3 4 5 6 7; do "$W/rb.$label" | grep -o 'scene640x360.*ms_per_frame=[0-9]*' | grep -o 'ms_per_frame=[0-9]*' ; done | sort | uniq -c | sort -k2 | tr '\n' ' ' | sed "s/^/RESULT $label raster_ms_hist /"; echo
    for sc in sunset noon night; do
        for i in 1 2 3 4 5 6 7; do "$W/sb.$label" | grep "_$sc " | grep -o 'us_per_frame=[0-9]*' | cut -d= -f2; done | sort -n | sed -n 4p | sed "s/^/RESULT $label sky_${sc}_us_median /"
    done
done
