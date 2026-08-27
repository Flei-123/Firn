#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
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
    local repeat="${3:-1}"
    python3 - "$ENGINE" "$file" "$label" "$repeat" <<'PY'
import os, struct, subprocess, sys, time
engine, path, label = sys.argv[1], sys.argv[2], sys.argv[3]
repeat = int(sys.argv[4]) if len(sys.argv) > 4 else 1
src = open(path, "rb").read()
blob = (struct.pack("<II", 0, len(src)) + src) * repeat
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

# ---------------------------------------------------------------- round 66
# The same measurement for the objects of round 66: a generator that is
# started and then ABANDONED in the middle of its body (frames, environment
# and closure stay behind), a promise with a reaction, and a BigInt of 600
# bits. If the frames of a suspended body were not ordinary GC objects,
# this run would grow without bound.
cat > "$WORK/gen.js" <<JS
var rounds = $ROUNDS;
function* work(i) {
  var acc = [i];
  try {
    for (var k = 0; k < 9; k++) { acc.push(k); yield k; }
  } finally { acc.length = 0; }
}
var checksum = 0;
for (var i = 0; i < rounds; i++) {
  // Started, ONE step, then dropped: the frame stack stays behind.
  var it = work(i);
  checksum += it.next().value;
  var p = new Promise(function (res) { res(i); });
  var b = (1n << 600n) + BigInt(i);
  checksum += Number(b & 1n);
}
print("gen", rounds, checksum);
JS

python3 - "$WORK/gen.js" "$WORK/genleak.js" <<'PY'
import sys
src = open(sys.argv[1]).read()
src = src.replace("var checksum = 0;", "var keep = [];\nvar checksum = 0;")
src = src.replace("  checksum += it.next().value;",
                  "  checksum += it.next().value;\n  keep.push(it);")
src = src.replace("  var p = new Promise(function (res) { res(i); });",
                  "  var p = new Promise(function (res) { res(i); });\n  keep.push(p);")
src = src.replace("  checksum += Number(b & 1n);",
                  "  checksum += Number(b & 1n);\n  keep.push(b);")
src = src.replace('print("gen", rounds, checksum);',
                  'print("genleak", keep.length, checksum);')
open(sys.argv[2], "w").write(src)
PY

echo
echo "== round 66: abandoned generators, promises, BigInts -- RSS stays flat =="
measure "$WORK/gen.js" "gen" | tee "$WORK/gen.log"
echo
echo "== the counter check: the same objects, held onto -- RSS MUST grow =="
GENLEAKR=$((ROUNDS / 10 + 2000))
sed -i "s|^var rounds = .*;|var rounds = $GENLEAKR;|" "$WORK/genleak.js"
measure "$WORK/genleak.js" "genleak" | tee "$WORK/genleak.log"

gen_growth=$(awk '/growth/ {print $(NF-1)}' "$WORK/gen.log" | tr -d '+')
genleak_growth=$(awk '/growth/ {print $(NF-1)}' "$WORK/genleak.log" | tr -d '+')
echo
echo "gen     growth: ${gen_growth} KiB"
echo "genleak growth: ${genleak_growth} KiB"
if [ "${gen_growth:-0}" -gt 8192 ]; then
    echo "FAILED: the generator run grew by more than 8 MiB -- that is a leak."
    exit 1
fi
MIN_GEN_LEAK=$(( ROUNDS / 40 + 4096 ))
if [ "${genleak_growth:-0}" -lt "$MIN_GEN_LEAK" ]; then
    echo "FAILED: the counter check grew by only ${genleak_growth} KiB (needed ${MIN_GEN_LEAK}) -- the measurement is broken."
    exit 1
fi
echo "OK: suspended generators are collected (${gen_growth} KiB), and a real leak is seen (${genleak_growth} KiB)."

# ------------------------------------------------------- the reaction jobs
# A promise reaction is a JOB, and the job queue is only drained at the END
# of a script (9.5) -- a program that hangs 20,000 reactions up and never
# lets them run holds them, and rightly so. So the measurement for the
# PROMISES is a different one: the same program as MANY JOBS in ONE process.
# Every job gets a fresh realm on the SAME heap, its queue is drained, and
# after that nothing of it may stay alive.
JOBR=$(( ROUNDS / 200 + 20 ))
JOBN=200
cat > "$WORK/jobs.js" <<JS
var rounds = $JOBR;
var acc = 0;
async function step(i) {
  var v = await i;
  return v + 1;
}
for (var i = 0; i < rounds; i++) {
  var p = new Promise(function (res) { res(i); });
  p.then(function (v) { return v + 1; }).then(function (v) { acc += v; });
  step(i).then(function (v) { acc += v; });
  Promise.all([1, Promise.resolve(2)]).then(function (a) { acc += a.length; });
}
print("jobs", rounds, acc);
JS
echo
echo "== the promises over $JOBN jobs: every queue is drained, RSS stays flat =="
measure "$WORK/jobs.js" "jobs" "$JOBN" | tee "$WORK/jobs.log"
jobs_growth=$(awk '/growth/ {print $(NF-1)}' "$WORK/jobs.log" | tr -d '+')
echo "jobs    growth: ${jobs_growth} KiB"
if [ "${jobs_growth:-0}" -gt 8192 ]; then
    echo "FAILED: the promise run grew by more than 8 MiB -- that is a leak."
    exit 1
fi
echo "OK: settled promises, reactions and async frames are collected (${jobs_growth} KiB)."

clean_growth=$(awk '/growth/ {print $(NF-1)}' "$WORK/clean.log" | tr -d '+')
leak_growth=$(awk '/growth/ {print $(NF-1)}' "$WORK/leak.log" | tr -d '+')
echo
echo "clean growth: ${clean_growth} KiB"
echo "leak  growth: ${leak_growth} KiB"
if [ "${clean_growth:-0}" -gt 8192 ]; then
    echo "FAILED: the clean run grew by more than 8 MiB -- that is a leak."
    exit 1
fi
MIN_LEAK=$(( ROUNDS / 40 + 4096 ))
if [ "${leak_growth:-0}" -lt "$MIN_LEAK" ]; then
    echo "FAILED: the counter check grew by only ${leak_growth} KiB (needed ${MIN_LEAK}) -- the measurement is broken."
    exit 1
fi
echo "OK: cycles are collected (${clean_growth} KiB), and a real leak is seen (${leak_growth} KiB)."
