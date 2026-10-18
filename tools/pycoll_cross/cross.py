#!/usr/bin/env python3
"""Cross-check std.pycoll against Python (tools/pycoll_cross/run.sh).

usage: cross.py TOOL WORKDIR SEED CASES
Random rows (region, amount, n) with many ties, empty and non-ASCII regions:
stable sorted(key=..., reverse=False/True) by float / int / bytes key, defaultdict(list)
group-by, Counter, sorted(items) and len/sum of values must match exactly.
"""
import os
import random
import subprocess
import sys

tool, work, seed, cases = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
rng = random.Random(seed)
os.makedirs(work, exist_ok=True)
bad = 0
REGIONS = ["", "a", "b", "ab", "Zed", "ä", "üß", "€", "\U0001f600", "north", "south", "x" * 40]
for c in range(cases):
    n = rng.randrange(0, 200)
    pool = rng.sample(REGIONS, rng.randrange(1, len(REGIONS) + 1))
    rows = []
    for i in range(n):
        rows.append((rng.choice(pool), rng.randrange(-30, 30) / 4.0, rng.randrange(-5, 5), i))
    path = os.path.join(work, "rows.tsv")
    with open(path, "w", encoding="utf-8") as f:
        for r, a, k, _ in rows:
            f.write(f"{r}\t{a}\t{k}\n")
    exp = []

    def ids(tag, lst):
        exp.append(tag + "".join(f" {x[3]}" for x in lst))

    ids("f+", sorted(rows, key=lambda r: r[1]))
    ids("f-", sorted(rows, key=lambda r: r[1], reverse=True))
    ids("i+", sorted(rows, key=lambda r: r[2]))
    ids("i-", sorted(rows, key=lambda r: r[2], reverse=True))
    ids("s+", sorted(rows, key=lambda r: r[0].encode()))
    ids("s-", sorted(rows, key=lambda r: r[0].encode(), reverse=True))
    groups = {}
    for r in rows:
        groups.setdefault(r[0], []).append(r)
    for k, g in groups.items():
        ids(f"g {k}\t", g)
    cnt = {k: len(g) for k, g in groups.items()}
    for k in sorted(cnt, key=lambda s: s.encode()):
        exp.append(f"c {k}\t{cnt[k]}")
    exp.append(f"values {len(cnt)} {sum(cnt.values())}")
    got = subprocess.run([tool, path], capture_output=True, timeout=120).stdout.decode("utf-8", "replace").split("\n")[:-1]
    if got != exp:
        bad += 1
        if bad <= 3:
            for i, (a, b) in enumerate(zip(got, exp)):
                if a != b:
                    print(f"case {c}: line {i}: got {a!r} want {b!r}")
                    break
            else:
                print(f"case {c}: length {len(got)} vs {len(exp)}")
print(f"pycoll_cross: {cases} cases, {bad} mismatches")
sys.exit(1 if bad else 0)
