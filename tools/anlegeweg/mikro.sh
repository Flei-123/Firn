#!/usr/bin/env bash
# tools/anlegeweg/mikro.sh -- the nanoseconds per elementary operation, three
# runs, BEST VALUE per line (the machine carries foreign load).
#
#   bash tools/anlegeweg/mikro.sh            with the collector running
#   bash tools/anlegeweg/mikro.sh --free     without collector work (limit raised)
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
[ -x "$FIRNC" ] || cargo build --release --manifest-path compiler/Cargo.toml
export FIRNLIB="$PWD/lib"
RUNS="${RUNS:-3}"
D="$(mktemp -d)"
trap 'rm -rf "$D"' EXIT
NAME="$D/aw_mikro"
if [ "${1:-}" = "--free" ]; then
    export FIRN_AW_UNLIMITED=1
fi
"$FIRNC" -o "$NAME" tools/anlegeweg/mikro.fi 2>&1 | grep -v 'RWX' || true
for i in $(seq 1 "$RUNS"); do
    "$NAME"
done | python3 -c '
import sys, re
best = {}
order = []
for line in sys.stdin:
    m = re.match(r"^(.*?)\s{2,}([\d.]+) ns$", line.rstrip())
    if not m:
        continue
    k, v = m.group(1).strip(), float(m.group(2))
    if k not in best:
        best[k] = v; order.append(k)
    else:
        best[k] = min(best[k], v)
for k in order:
    print("%-34s %8.3f ns" % (k, best[k]))
'
