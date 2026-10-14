#!/usr/bin/env bash
# tools/mc/perf/stream_scale.sh [lib-dir] [opt-level] -- scaling of the mesh workers: stream_bench with 0 (inline) / 1 / 2 / 3 / 4 workers, interleaved
# rounds (ROUNDS=3), median wall time of the load from empty to fully meshed. Start via heavy; the server load makes the numbers noisy.
set -uo pipefail
cd "$(dirname "$0")/../../.."
LIB=${1:-$(pwd)/lib}; OPT=${2:-release-fast}; ROUNDS=${ROUNDS:-3}
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
FIRNLIB="$LIB" $FIRNC --opt-level="$OPT" -o "$W/b" tools/mc/perf/stream_bench.fi || exit 1
echo "load $(cut -d' ' -f1-3 /proc/loadavg), rad ${SB_RAD:-4}, runs ${SB_RUNS:-5}, rounds $ROUNDS, $OPT"
for r in $(seq 1 "$ROUNDS"); do for n in 0 1 2 3 4; do
    SB_WORKERS=$n SB_RAD=${SB_RAD:-4} SB_RUNS=${SB_RUNS:-5} "$W/b" 2>&1 | awk -v n=$n -v r=$r '$2=="wall_ms"||$2=="wall_ms_min"||$2=="mesh_cpu_ms"||$2=="submit_us_per_section"{print n, r, $2, $3}' >> "$W/res.txt"
done; done
python3 -I - "$W/res.txt" <<'P'
import sys, statistics
d = {}
for l in open(sys.argv[1]):
    n, r, k, v = l.split(); d.setdefault((int(n), k), []).append(float(v))
base = statistics.median(d[(1, "wall_ms")])
for n in range(5):
    print(f"  workers {n}: " + "  ".join(f"{k} {statistics.median(d[(n,k)]):.1f}" for k in ("wall_ms", "wall_ms_min", "mesh_cpu_ms", "submit_us_per_section")) + f"  wall speedup vs 1 worker x{base/statistics.median(d[(n,'wall_ms')]):.2f}")
P
