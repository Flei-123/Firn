#!/usr/bin/env bash
# tools/mc/perf/mesh_ab.sh <old-lib-dir> [opt-level] [rounds] -- A/B of the mesher micro benchmark (mesh_bench.fi): the old and
# the new lib are built with the same compiler / opt level and run interleaved (same load window); median over the rounds of the
# per-mode p50. Output hashes of old and new must be equal (bit-identical meshes).
set -uo pipefail
cd "$(dirname "$0")/../../.."
OLD=${1:?old lib dir}; OPT=${2:-release-fast}; ROUNDS=${3:-5}
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
FIRNLIB="$OLD" $FIRNC --opt-level="$OPT" -o "$W/old" tools/mc/perf/mesh_bench.fi || exit 1
FIRNLIB="$(pwd)/lib" $FIRNC --opt-level="$OPT" -o "$W/new" tools/mc/perf/mesh_bench.fi || exit 1
for i in $(seq 1 "$ROUNDS"); do
    "$W/old" "${REPS:-9}" > "$W/o$i.txt"; "$W/new" "${REPS:-9}" > "$W/n$i.txt"
done
python3 -I - "$W" "$ROUNDS" <<'P'
import sys, statistics, re
w, n = sys.argv[1], int(sys.argv[2])
def load(p):
    d = {}
    for l in open(p):
        m = re.match(r"RESULT (\S+) (\S+) (.*)", l)
        if m: d.setdefault((m.group(1), m.group(3)), []).append(float(m.group(2)))
    return d
o = [load(f"{w}/o{i}.txt") for i in range(1, n + 1)]
nw = [load(f"{w}/n{i}.txt") for i in range(1, n + 1)]
ok = True
for k in o[0]:
    if k[1] == "hash":
        same = o[0][k] == nw[0][k]
        ok &= same
        print(f"  {k[0]:18s} hash {'EQUAL' if same else 'DIFFERENT'}")
    elif k[1].endswith("p50"):
        a = [x[k][0] for x in o]; b = [x[k][0] for x in nw]
        ma, mb = statistics.median(a), statistics.median(b)
        print(f"  {k[0]:18s} p50 old {ma:8.1f} us [{min(a):.0f}..{max(a):.0f}]  new {mb:8.1f} us [{min(b):.0f}..{max(b):.0f}]  x{ma/mb:.2f}")
print("RESULT mesh_ab:", "ok" if ok else "FAIL")
P
