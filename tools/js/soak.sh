#!/usr/bin/env bash
# tools/js/soak.sh -- the ENDURANCE RUN with the counter check.
#
# THE MEASUREMENT. A JavaScript program builds cycles that no reference
# count can ever release -- an object that holds a closure which holds the
# object back, plus a prototype chain and a Map that holds both. It does
# that N times in a row. The RSS of the process is sampled while it runs;
# it must stay FLAT.
#
# THE COUNTER CHECK. The same program, but every set is additionally hung
# onto a global array. Now it MUST grow. Without that second run the first
# one proves nothing: a measurement that cannot see a leak is not a
# measurement.
#
# Usage:  bash tools/js/soak.sh [engine] [rounds]
set -euo pipefail
cd "$(dirname "$0")/../.."
ENGINE="${1:-.js-work/jsrun}"
ROUNDS="${2:-40000}"
WORK=".js-work/soak"
mkdir -p "$WORK"

cat > "$WORK/clean.js" <<JS
var rounds = $ROUNDS;
function makeSet(i) {
  // The cycle: node <-> closure <-> node, plus a prototype chain and a Map
  // that holds keys AND values. Every reference is strong.
  var node = { id: i, kids: [] };
  node.self = function () { return node.id; };
  var child = Object.create(node);
  child.parent = node;
  node.kids.push(child);
  var m = new Map();
  m.set(node, child);
  m.set(child, node);
  node.table = m;
  child.table = m;
  return node;
}
var checksum = 0;
for (var i = 0; i < rounds; i++) {
  var n = makeSet(i);
  checksum += n.self() - n.kids[0].parent.id;
}
print("clean", rounds, checksum);
JS

python3 - "$WORK/clean.js" "$WORK/leak.js" <<'PY'
import sys
src = open(sys.argv[1]).read()
src = src.replace("var checksum = 0;", "var keep = [];\nvar checksum = 0;")
src = src.replace("  var n = makeSet(i);", "  var n = makeSet(i);\n  keep.push(n);")
src = src.replace('print("clean", rounds, checksum);',
                  'print("leak", keep.length, checksum);')
open(sys.argv[2], "w").write(src)
PY

measure() {
    local file="$1"
    local label="$2"
    python3 - "$ENGINE" "$file" "$label" <<'PY'
import os, struct, subprocess, sys, time
engine, path, label = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(path, "rb").read()
blob = struct.pack("<II", 0, len(src)) + src
p = subprocess.Popen([engine], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
p.stdin.write(blob)
p.stdin.close()
lo = None
hi = 0
first = None
samples = 0
t0 = time.time()
while p.poll() is None:
    try:
        with open("/proc/%d/status" % p.pid) as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    kb = int(line.split()[1])
                    if samples > 3:          # skip the start up
                        if first is None:
                            first = kb
                        lo = kb if lo is None else min(lo, kb)
                        hi = max(hi, kb)
                    samples += 1
                    break
    except FileNotFoundError:
        break
    time.sleep(0.05)
out = p.stdout.read().decode("utf-8", "replace")
dt = time.time() - t0
if first is None:
    first = hi
print("%-6s rc=%d  %5.1fs  RSS first %6d KiB  max %6d KiB  growth %+6d KiB" %
      (label, p.returncode, dt, first, hi, hi - first))
print("       output: %s" % out.split("\x00")[0].strip().replace("\n", " | "))
sys.exit(0 if p.returncode == 0 else 1)
PY
}

echo "== the clean run: cycles are collected, RSS stays flat =="
measure "$WORK/clean.js" "clean" | tee "$WORK/clean.log"
echo
echo "== the counter check: the same graph, held onto -- RSS MUST grow =="
# The counter check only has to SHOW the growth, not run as long: it holds
# everything, so every collection walks a live set that keeps growing.
LEAKR=$((ROUNDS / 10 + 2000))
sed -i "s|^var rounds = .*;|var rounds = $LEAKR;|" "$WORK/leak.js"
measure "$WORK/leak.js" "leak" | tee "$WORK/leak.log"

clean_growth=$(awk '/growth/ {print $(NF-1)}' "$WORK/clean.log" | tr -d '+')
leak_growth=$(awk '/growth/ {print $(NF-1)}' "$WORK/leak.log" | tr -d '+')
echo
echo "clean growth: ${clean_growth} KiB"
echo "leak  growth: ${leak_growth} KiB"
if [ "${clean_growth:-0}" -gt 8192 ]; then
    echo "FAILED: the clean run grew by more than 8 MiB -- that is a leak."
    exit 1
fi
if [ "${leak_growth:-0}" -lt 16384 ]; then
    echo "FAILED: the counter check did NOT grow -- the measurement is broken."
    exit 1
fi
echo "OK: cycles are collected (${clean_growth} KiB), and a real leak is seen (${leak_growth} KiB)."
