#!/usr/bin/env bash
# tools/gc_bench/prof.sh -- ROUND SAMMLER: a callgrind profile with REAL
# function names.
#
# callgrind cannot resolve the symbols of a firnc binary (static, no DWARF,
# RWX LOAD segment), so it records raw addresses. This script reads the RAW
# callgrind output, sums the SELF cost per function entry (cost lines after
# an `fn=` header, NOT the `calls=`/`cfn=` lines, which are inclusive), and
# maps each function's address onto the symbol whose range contains it.
#
# Usage: bash tools/gc_bench/prof.sh <binary> <callgrind-out> [top-n]
set -uo pipefail
BIN="$1"
OUT="$2"
TOP="${3:-20}"

python3 - "$BIN" "$OUT" "$TOP" <<'PY'
import subprocess, sys, re, collections, bisect
binary, out, top = sys.argv[1], sys.argv[2], int(sys.argv[3])

syms = []
for line in subprocess.run(["nm", "-n", binary], capture_output=True, text=True).stdout.splitlines():
    p = line.split()
    if len(p) >= 3 and re.fullmatch(r"[0-9a-fA-F]+", p[0]) and p[1] in "TtWw":
        syms.append((int(p[0], 16), p[2]))
syms.sort()
addrs = [s[0] for s in syms]

def name_of(a):
    i = bisect.bisect_right(addrs, a) - 1
    return syms[i][1] if i >= 0 else "?"

names = {}          # fn id -> raw name
self_cost = collections.Counter()
cur = None
skip_next_cost = False   # the cost line following `calls=` is the CALLEE's

for line in open(out, errors="replace"):
    line = line.rstrip("\n")
    if not line or line.startswith("#"):
        continue
    m = re.match(r"^fn=\((\d+)\)(?:\s+(.*))?$", line)
    if m:
        cur = m.group(1)
        if m.group(2):
            names[cur] = m.group(2)
        skip_next_cost = False
        continue
    if line.startswith("calls="):
        skip_next_cost = True
        continue
    m = re.match(r"^cfn=\((\d+)\)(?:\s+(.*))?$", line)
    if m:
        if m.group(2):
            names[m.group(1)] = m.group(2)
        continue
    if re.match(r"^(cfn|cfl|cfi|fl|fi|fe|ob|cob)=", line):
        continue
    if re.match(r"^(positions|events|summary|totals|version|creator|cmd|part|desc|pid)", line):
        continue
    # a cost line: <position> <cost> ...
    m = re.match(r"^([0-9a-fA-FxX+*\-]+)\s+(\d+)", line)
    if m and cur is not None:
        if skip_next_cost:
            skip_next_cost = False
            continue
        self_cost[cur] += int(m.group(2))

agg = collections.Counter()
for fid, c in self_cost.items():
    nm_ = names.get(fid, "?")
    ma = re.search(r"0x([0-9a-fA-F]+)", nm_)
    if ma:
        nm_ = name_of(int(ma.group(1), 16))
    agg[nm_] += c

total = sum(agg.values())
print(f"total Ir (self): {total:,}")
print(f"{'%':>7}  {'Ir':>15}  function")
for n, c in agg.most_common(top):
    print(f"{100.0*c/total:7.2f}  {c:15,}  {n}")

gc = sum(c for n, c in agg.items() if "__gc" in n or n.startswith("_F0.gc_"))
print(f"\nCOLLECTOR SHARE: {100.0*gc/total:.2f} %  ({gc:,} / {total:,} Ir)")
PY
