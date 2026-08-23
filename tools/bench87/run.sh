#!/usr/bin/env bash
# tools/bench87/run.sh -- the two throughputs of round 87 that round 82 left
# lying, and the price for each of them.
#
#   1. DEFLATE level 6 against `gzip -6` -- on REAL data, not on a generated
#      stream: the same file whose compression ratio BENCHMARKS.md quotes.
#      Both numbers together, because a compressor that gets faster by
#      packing worse has not got faster.
#   2. JSON reading, one document with integers and one with floats. Same
#      shape, same number of members; only the numbers differ.
#
# Every number is the BEST of several passes -- on a shared machine the
# average measures the neighbours.
#
#   bash tools/bench87/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT="$(pwd)"
export FIRNLIB="$ROOT/lib"
FIRNC=compiler/target/release/firnc
W=.bench87-work
mkdir -p "$W"
RUNS=${BENCH87_RUNS:-5}

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml >/dev/null 2>&1 \
        || { echo "FAIL: the compiler does not build"; exit 1; }
fi

echo "== the machine =="
echo "  $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//')"
echo "  $(date -u '+%Y-%m-%d %H:%M UTC')"
echo

# --------------------------------------------------------------- 1. deflate
#
# NOT one number but a TABLE over the levels, and that is the point of this
# section. "DEFLATE is 1.88x behind gzip -6" compares two level SIXES, and
# the two level sixes are not the same algorithm: Firn packs 150,357 octets
# on this file where gzip packs 151,856. The honest question is what the same
# OUTPUT SIZE costs on each side, and only a table can answer it.
echo "== DEFLATE: the levels against each other =="
"$FIRNC" --opt-level=release-fast -o "$W/deflatespeed" tools/bench87/deflatespeed.fi \
    2>"$W/build.log" || { echo "FAIL: deflatespeed"; tail -20 "$W/build.log"; exit 1; }

F=${BENCH87_FILE:-testdata/realweb/wikipedia_en_rust.html}
REPS=${BENCH87_REPS:-3}
SIZE=$(stat -c%s "$F")
echo "  corpus: $F ($SIZE octets), best of $RUNS passes, $REPS per pass"
echo "  level       Firn MiB/s   Firn octets   gzip MiB/s   gzip octets"
for L in 1 4 6 9; do
    best=999999999
    packed=0
    for r in $(seq 1 $RUNS); do
        "$W/deflatespeed" "$F" "$L" "$REPS" "$W/r.txt" >/dev/null 2>&1 || { echo "  FAIL level $L"; continue; }
        read -r us pk < "$W/r.txt"
        if [ "$us" -lt "$best" ]; then best=$us; packed=$pk; fi
    done
    python3 tools/bench87/gzip_row.py "$L" "$SIZE" "$REPS" "$best" "$packed" "$F" "$RUNS"
done
echo

# ------------------------------------------------------------------ 2. json
echo "== JSON reading =="
python3 tools/bench87/gen_json.py "$W/json" 20000 | sed 's/^/  /'
"$FIRNC" --opt-level=release-fast -o "$W/jsonspeed" tools/bench87/jsonspeed.fi \
    2>>"$W/build.log" || { echo "FAIL: jsonspeed"; tail -20 "$W/build.log"; exit 1; }
json_row() { # <name> <file> <reps>
    local name="$1" f="$2" reps="$3" best=999999999 v r
    for r in $(seq 1 $RUNS); do
        "$W/jsonspeed" "$f" "$reps" "$W/t.txt" >/dev/null 2>&1 || { echo "  FAIL $name"; return 1; }
        v=$(cat "$W/t.txt")
        [ "$v" -lt "$best" ] && best=$v
    done
    python3 - "$name" "$f" "$reps" "$best" <<'PY'
import sys, os
name, f, reps, us = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
mb = os.path.getsize(f) * reps / 1048576.0
print("  %-12s %8.2f MiB/s   (%d bytes x %d in %.1f ms)"
      % (name, mb / (us / 1e6), os.path.getsize(f), reps, us / 1000.0))
PY
}
json_row "integers" "$W/json/int.json" 5
json_row "floats"   "$W/json/float.json" 5
python3 - "$W/json/int.json" "$W/json/float.json" <<'PY'
import sys
print("  (the two documents have the same shape; only the numbers differ)")
PY
