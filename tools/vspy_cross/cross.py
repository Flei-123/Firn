#!/usr/bin/env python3
"""Cross-check std.dict and fsx.find against Python (tools/vspy_cross/run.sh).

usage: cross.py TOOL WORKDIR SEED CASES
  * dict:  CASES random op sequences (set/inc/rm/get, colliding keys, empty key,
           non-ASCII bytes, long keys, remove + re-add) against a Python dict:
           every result line, the insertion-order dump, sorted keys and the top 7
           (Counter.most_common order: value desc, key asc).
  * find:  CASES/10 random trees against sorted(os.walk + fnmatch + size + mtime).
Exit 0 and prints the mismatch count (target 0).
"""
import fnmatch
import os
import random
import subprocess
import sys

tool, work, seed, cases = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
rng = random.Random(seed)
os.makedirs(work, exist_ok=True)
bad = 0
checked = 0


def run(args):
    return subprocess.run([tool] + args, capture_output=True, timeout=120).stdout


def rand_key():
    k = rng.randrange(8)
    if k == 0:
        return ""
    if k == 1:
        return "k" * rng.randrange(40, 400)
    if k == 2:
        return rng.choice(["ä", "üß", "€", "\U0001f600"]) + str(rng.randrange(5))
    return rng.choice("abcde") + str(rng.randrange(0, 12 if k < 6 else 400))


for c in range(cases):
    n = rng.randrange(1, 300)
    lines, exp, d = [], [], {}
    for _ in range(n):
        op = rng.choice(["set", "inc", "inc", "rm", "get", "get"])
        key = rand_key()
        if op == "set":
            v = rng.randrange(-1000, 1000)
            lines.append(f"set\t{key}\t{v}")
            d[key] = v
            exp.append("-")
        elif op == "inc":
            v = rng.randrange(-5, 50)
            lines.append(f"inc\t{key}\t{v}")
            d[key] = d.get(key, 0) + v
            exp.append(str(d[key]))
        elif op == "rm":
            lines.append(f"rm\t{key}")
            exp.append("true" if key in d else "false")
            d.pop(key, None)
        else:
            lines.append(f"get\t{key}")
            exp.append(str(d[key]) if key in d else "none")
    exp.append(f"len {len(d)}")
    for k, v in d.items():
        exp.append(f"item {k}\t{v}")
    for k in sorted(d, key=lambda s: s.encode()):
        exp.append(f"sorted {k}")
    top = sorted(d.items(), key=lambda kv: (-kv[1], kv[0].encode()))[:7]
    for k, v in top:
        exp.append(f"top {k}\t{v}")
    path = os.path.join(work, "ops.txt")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    got = run(["dict", path]).decode("utf-8", "replace").split("\n")[:-1]
    checked += 1
    if got != exp:
        bad += 1
        if bad <= 3:
            for i, (a, b) in enumerate(zip(got, exp)):
                if a != b:
                    print(f"dict case {c}: line {i}: got {a!r} want {b!r}")
                    break
            else:
                print(f"dict case {c}: length {len(got)} vs {len(exp)}")

for c in range(max(1, cases // 10)):
    root = os.path.join(work, f"tree{c}")
    os.makedirs(root, exist_ok=True)
    names = []
    for i in range(rng.randrange(1, 40)):
        depth = rng.randrange(0, 4)
        sub = "/".join(rng.choice(["a", "b", "c d", "x"]) for _ in range(depth))
        d = os.path.join(root, sub)
        os.makedirs(d, exist_ok=True)
        name = rng.choice(["f", "g", "old_h", "readme", ".hid"]) + str(rng.randrange(6)) + rng.choice([".txt", ".log", ".md", ""])
        p = os.path.join(d, name)
        with open(p, "wb") as f:
            f.write(b"x" * rng.randrange(0, 900))
        os.utime(p, (1000000 + rng.randrange(0, 5000), 1000000 + rng.randrange(0, 5000)))
    pat = rng.choice(["*.txt", "*", "old_*", "?[0-3].*", "f*", "*.md", "nomatch"])
    min_size = rng.choice([0, 100, 500, 899])
    max_ns = rng.choice([1 << 62, 1002500 * 10**9, 1000000 * 10**9])
    exp = []
    for dp, dn, fn in os.walk(root):
        for f in fn:
            p = os.path.join(dp, f)
            st = os.stat(p)
            if fnmatch.fnmatchcase(f, pat) and st.st_size >= min_size and st.st_mtime_ns < max_ns:
                exp.append(os.path.relpath(p, root))
    exp = sorted(exp, key=lambda s: os.path.join(root, s).encode())
    got = run(["find", root, pat, str(min_size), str(max_ns)]).decode().split("\n")[:-1]
    checked += 1
    if got != [f"count {len(exp)}"] + exp:
        bad += 1
        if bad <= 3:
            print(f"find case {c}: {pat!r} {min_size} {max_ns}: got {len(got) - 1} want {len(exp)}")

print(f"vspy_cross: {checked} cases, {bad} mismatches")
sys.exit(1 if bad else 0)
