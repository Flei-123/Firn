#!/usr/bin/env python3
"""Cross-check of text.merge3 against `git merge-file` (byte for byte).

usage: merge_cross.py MERGETOOL WORKDIR CASES SEED [sabotage]

For CASES random (base, ours, theirs) triples the tool and `git merge-file -p -L ours -L base -L theirs`
must give the same bytes and the same conflict count, in several variants (merge / diff3 / zdiff3 style,
--ours / --theirs / --union, --marker-size). `sabotage` runs git with another marker size on purpose: the
run must then REPORT mismatches (counter-check of the harness).
Exit 1 when any case differs.
"""
import os
import random
import subprocess
import sys

tool, work, cases, seed = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
sabotage = len(sys.argv) > 5 and sys.argv[5] == "sabotage"
rnd = random.Random(seed)

VOCAB = ["", "a", "b", "c", "}", "{", ");", "int x;", "return 0;", "  foo();", "\tbar();", "x = 1", "# comment",
         "café", "-- dash", "+ plus", "<<<<<<< fake", "=======", ">>>>>>> fake", " ", "  ", "end", "...", "//"]


def rand_line(pool):
    r = rnd.random()
    if r < 0.2:
        return "line %d" % rnd.randrange(1000)
    if r < 0.3:
        return "-" * rnd.randrange(1, 5)
    return rnd.choice(pool)


def rand_base():
    n = rnd.choice([0, 1, 2, 3, 4, 6, 8, 12, 20, 40, 80, 150])
    if rnd.random() < 0.03:
        n = rnd.choice([400, 900, 2500, 6000])
    mode = rnd.random()
    if mode < 0.3:
        return [rand_line(VOCAB) for _ in range(n)]
    if mode < 0.55:
        return ["line %d" % i for i in range(n)]
    if mode < 0.8:
        return [rnd.choice(VOCAB[:8]) for _ in range(n)]
    pool = ["x", "y", "}", "", "{"]
    return [rnd.choice(pool) for _ in range(n)]


def mutate(lines, near=None):
    out = list(lines)
    k = rnd.choice([0, 1, 1, 2, 2, 3, 4, 6, 10, 25])
    if len(lines) > 600:
        k = rnd.choice([5, 40, 200, 600, 1500, 3000])
    for _ in range(k):
        t = rnd.random()
        pos = rnd.randrange(len(out) + 1)
        if near is not None and rnd.random() < 0.6:
            pos = max(0, min(len(out), near + rnd.randrange(-4, 5)))
        if t < 0.3 and out:
            del out[min(pos, len(out) - 1)]
        elif t < 0.6:
            out.insert(pos, rand_line(VOCAB))
        elif t < 0.8 and out:
            out[min(pos, len(out) - 1)] = rand_line(VOCAB)
        elif out:
            # copy a block to another place
            a = rnd.randrange(len(out))
            b = min(len(out), a + rnd.randrange(1, 6))
            out[pos:pos] = out[a:b]
    return out


def to_bytes(lines, crlf=False, final_newline=True):
    if not lines:
        return b""
    eol = "\r\n" if crlf else "\n"
    s = eol.join(lines) + (eol if final_newline else "")
    return s.encode("utf-8")


def make_case():
    base = rand_base()
    near = rnd.randrange(len(base) + 1) if rnd.random() < 0.7 else None
    ours = mutate(base, near)
    theirs = mutate(base, near)
    if rnd.random() < 0.08:
        theirs = list(ours)  # identical change
        if theirs and rnd.random() < 0.5:
            theirs[rnd.randrange(len(theirs))] = "diff %d" % rnd.randrange(9)
    crlf = [rnd.random() < 0.07 for _ in range(3)]
    fin = [rnd.random() > 0.12 for _ in range(3)]
    if rnd.random() < 0.5:
        crlf = [crlf[0]] * 3
    return (to_bytes(base, crlf[0], fin[0]), to_bytes(ours, crlf[1], fin[1]), to_bytes(theirs, crlf[2], fin[2]))


os.makedirs(work, exist_ok=True)
data = []
for i in range(cases):
    b, o, t = make_case()
    for name, content in (("base", b), ("ours", o), ("theirs", t)):
        with open("%s/%d.%s" % (work, i, name), "wb") as f:
            f.write(content)
    data.append((b, o, t))

# style, favor, marker size, every n-th case
VARIANTS = [(0, 0, 7, 1), (1, 0, 7, 2), (2, 0, 7, 2), (0, 1, 7, 3), (0, 2, 7, 3), (0, 3, 7, 3), (1, 3, 9, 4), (2, 2, 3, 4)]
STYLE_FLAG = {0: [], 1: ["--diff3"], 2: ["--zdiff3"]}
FAVOR_FLAG = {0: [], 1: ["--ours"], 2: ["--theirs"], 3: ["--union"]}
total_bad = 0
total_cases = 0
total_conflict = 0
for style, favor, marker, step in VARIANTS:
    r = subprocess.run([tool, "batch", work, str(cases), str(style), str(favor), str(marker)], capture_output=True)
    if r.returncode != 0:
        print("  FAIL  tool batch failed rc=%d %s" % (r.returncode, r.stderr[:200]))
        sys.exit(1)
    conflicts = {}
    for line in r.stdout.decode().split("\n"):
        if line:
            i, c = line.split()
            conflicts[int(i)] = int(c)
    bad = 0
    ncon = 0
    first = None
    for i in range(0, cases, step):
        gm = marker + 2 if sabotage and style == 0 and favor == 0 else marker
        cmd = ["git", "merge-file", "-p", "-L", "ours", "-L", "base", "-L", "theirs", "--marker-size", str(gm)] \
            + STYLE_FLAG[style] + FAVOR_FLAG[favor] + ["%s/%d.ours" % (work, i), "%s/%d.base" % (work, i), "%s/%d.theirs" % (work, i)]
        g = subprocess.run(cmd, capture_output=True)
        with open("%s/%d.out" % (work, i), "rb") as f:
            mine = f.read()
        gconf = min(g.returncode, 127) if g.returncode >= 0 else -1
        ok = mine == g.stdout and min(conflicts[i], 127) == gconf
        if g.returncode > 0:
            ncon += 1
        if not ok:
            bad += 1
            if first is None:
                first = i
                with open("%s/%d.git" % (work, i), "wb") as f:
                    f.write(g.stdout)
    total_bad += bad
    total_cases += len(range(0, cases, step))
    total_conflict += ncon
    print("variant style=%d favor=%d marker=%d: %d cases, %d with conflicts, %d mismatches%s" % (
        style, favor, marker, len(range(0, cases, step)), ncon, bad, ("" if first is None else " (first: case %d, files %s/%d.*)" % (first, work, first))))
print("merge_cross: %d cases, %d with conflicts, %d mismatches" % (total_cases, total_conflict, total_bad))
sys.exit(1 if total_bad else 0)
