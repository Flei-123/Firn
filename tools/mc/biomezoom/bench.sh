#!/usr/bin/env bash
# tools/mc/biomezoom/bench.sh -- build + run bench.fi (mesher cost of the biome zoom). Start it via /root/jarvis/bin/heavy. Prints BENCH lines + load.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
$FIRNC -o "$W/bench" tools/mc/biomezoom/bench.fi 2>"$W/err" || { head -20 "$W/err"; echo "compile failed"; exit 1; }
echo "load_before=$(cut -d' ' -f1-3 /proc/loadavg)"
"$W/bench"
echo "load_after=$(cut -d' ' -f1-3 /proc/loadavg)"
