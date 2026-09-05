#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/js/r74soak.sh -- the ENDURANCE RUN for the objects of round 74,
# with the counter check.
#
# THE MEASUREMENT. A program builds, over and over, exactly the things this
# round adds and drops them again: a COMPILED PATTERN (`val.JsRegExp` with
# its two raw vectors of nodes and ranges), the ITERATOR objects of Array,
# String, Map and Set, a DATE, a WeakMap over a live key, a set operation
# and the result list of a `matchAll`. The RSS of the process is sampled
# while it runs; it must stay FLAT.
#
# THE COUNTER CHECK. The same program, but everything is additionally hung
# onto a global array. Now it MUST grow. Without that second run the first
# one proves nothing: a measurement that cannot see a leak is not a
# measurement.
#
# Usage:  bash tools/js/r74soak.sh [engine] [rounds]
set -euo pipefail
cd "$(dirname "$0")/../.."
ENGINE="${1:-.js-work/jsrun}"
ROUNDS="${2:-20000}"
WORK=".js-work/r74soak"
mkdir -p "$WORK"

cat > "$WORK/clean.js" <<JS
var rounds = $ROUNDS;
function makeSet(i) {
  // A pattern with two groups, compiled anew every round -- the node tree
  // and the range table are raw vectors in the GC heap.
  var re = new RegExp("(a+)(b|c)" + (i % 7), "g");
  var hit = re.exec("xaaab" + (i % 7));
  // The iterators of the ordinary collections, each advanced one step and
  // then dropped in mid-flight.
  var ai = [1, 2, 3][Symbol.iterator](); ai.next();
  var si = "abc"[Symbol.iterator](); si.next();
  var m = new Map([[i, { n: i }]]);
  var mi = m.entries(); mi.next();
  var w = new WeakMap(); w.set(m, i);
  var d = new Date(i * 1000);
  var s = new Set([i, i + 1]).union(new Set([i + 2]));
  var all = [..."a1b2c3".matchAll(/\w(\d)/g)];
  return (hit ? hit[1].length : 0) + s.size + all.length + (d.getTime() % 2);
}
var checksum = 0;
for (var i = 0; i < rounds; i++) {
  checksum += makeSet(i);
}
print("clean", rounds, checksum);
JS

python3 - "$WORK/clean.js" "$WORK/leak.js" <<'PY'
import sys
src = open(sys.argv[1]).read()
src = src.replace("var checksum = 0;", "var keep = [];\nvar checksum = 0;")
src = src.replace("  checksum += makeSet(i);",
                  "  checksum += makeSet(i);\n"
                  "  keep.push(new RegExp('(a+)(b|c)' + (i % 7), 'g'));\n"
                  "  keep.push(new Date(i * 1000));\n"
                  "  keep.push([..'x'.matchAll(/x/g)]);".replace("..", "..."))
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

echo "== the clean run: the objects of round 74 are collected, RSS stays flat =="
measure "$WORK/clean.js" "clean" | tee "$WORK/clean.log"
echo
echo "== the counter check: the same objects, held onto -- RSS MUST grow =="
LEAKR=$((ROUNDS / 6 + 2000))
sed -i "s|^var rounds = .*;|var rounds = $LEAKR;|" "$WORK/leak.js"
measure "$WORK/leak.js" "leak" | tee "$WORK/leak.log"

CLEAN=$(awk '/^clean /{print $(NF-1)}' "$WORK/clean.log")
LEAK=$(awk '/^leak /{print $(NF-1)}' "$WORK/leak.log")
echo
echo "   clean growth : ${CLEAN} KiB"
echo "   leak  growth : ${LEAK} KiB"
if [ "${CLEAN#+}" -gt 24000 ]; then
    echo "FAILED: the clean run grew -- that is a leak."
    exit 1
fi
if [ "${LEAK#+}" -lt 2000 ]; then
    echo "FAILED: the counter check did NOT grow -- the measurement is blind."
    exit 1
fi
echo "OK: flat where it has to be flat, growing where it has to grow."
