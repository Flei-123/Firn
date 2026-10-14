#!/usr/bin/env bash
# tools/mc/perf/stream_ab.sh <old-lib-dir> [opt-level] [rounds] -- A/B of tools/mc/perf/stream_bench.fi (render thread cost of
# snapshot + submit, mesh worker scaling). Old and new build run interleaved; SB_WORKERS / SB_RUNS / SB_RAD pass through.
# Prints the median over the rounds of each RESULT line for old and new.
set -uo pipefail
cd "$(dirname "$0")/../../.."
OLD=${1:?old lib dir}; OPT=${2:-release-fast}; ROUNDS=${3:-3}
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
FIRNLIB="$OLD" $FIRNC --opt-level="$OPT" -o "$W/old" tools/mc/perf/stream_bench.fi || exit 1
FIRNLIB="$(pwd)/lib" $FIRNC --opt-level="$OPT" -o "$W/new" tools/mc/perf/stream_bench.fi || exit 1
for i in $(seq 1 "$ROUNDS"); do
    "$W/old" > "$W/o$i.txt" 2>&1; "$W/new" > "$W/n$i.txt" 2>&1
done
echo "load $(cut -d' ' -f1-3 /proc/loadavg), workers ${SB_WORKERS:-4}, runs ${SB_RUNS:-5}, rad ${SB_RAD:-3}, $OPT"
python3 -I - "$W" "$ROUNDS" <<'P'
import sys, statistics, re
w, n = sys.argv[1], int(sys.argv[2])
def load(p):
    d = {}
    for l in open(p):
        m = re.match(r"RESULT (\S+) (\S+) (.*)", l)
        if m: d.setdefault(m.group(1), []).append(float(m.group(2)))
    return d
o = [load(f"{w}/o{i}.txt") for i in range(1, n + 1)]
nw = [load(f"{w}/n{i}.txt") for i in range(1, n + 1)]
for k in o[0]:
    a = [x[k][0] for x in o]; b = [x[k][0] for x in nw]
    ma, mb = statistics.median(a), statistics.median(b)
    print(f"  {k:28s} old {ma:10.3f} [{min(a):.3f}..{max(a):.3f}]  new {mb:10.3f} [{min(b):.3f}..{max(b):.3f}]  x{(ma/mb if mb else 0):.2f}")
P
