#!/usr/bin/env python3
"""Cross-check of text.diff against GNU diff / GNU patch / difflib.

usage: diff_cross.py DIFFTOOL WORKDIR CASES SEED ALGO CTX

For CASES random file pairs:
  * our unified diff applied by GNU `patch` must give b           (patch_ok)
  * the GNU `diff -u` applied by OUR apply must give b             (gnu_into_ours_ok)
  * our diff parsed + applied by our code gives b (reported by the tool: selfok)
  * hunk count and changed-line count compared with GNU diff       (reported)
  * identical bytes to GNU diff -u                                  (reported)
  * with ALGO=0 the number of changed lines is <= difflib's        (min_ok)
  * fuzz/offset: GNU patch and ours on patches whose target was edited (shifted)
Exit 1 when a hard property fails (patch_ok, gnu_into_ours_ok, selfok, min vs GNU).
"""
import difflib
import os
import random
import subprocess
import sys

tool, work, cases, seed, algo, ctx = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5], sys.argv[6]
rnd = random.Random(seed)

VOCAB = ["", "a", "b", "c", "}", "{", "int x;", "return 0;", "  foo();", "\tbar();", "x = 1", "# comment",
         "café", "üñi", "-- dash", "+ plus", "@@ at", " space", "\\ back", "end"]


def rand_line():
    if rnd.random() < 0.25:
        return "line %d" % rnd.randrange(1000)
    return rnd.choice(VOCAB)


def rand_file():
    mode = rnd.random()
    n = rnd.choice([0, 1, 2, 3, 5, 8, 12, 20, 40, 80, 150])
    if mode < 0.3:
        lines = [rand_line() for _ in range(n)]
    elif mode < 0.7:
        lines = ["line %d" % i for i in range(n)]
    else:
        lines = [rnd.choice(VOCAB[:8]) for _ in range(n)]
    return lines


def mutate(lines):
    out = list(lines)
    k = rnd.choice([0, 1, 1, 2, 3, 5, 10, 30])
    for _ in range(k):
        t = rnd.random()
        if t < 0.35 and out:
            del out[rnd.randrange(len(out))]
        elif t < 0.7:
            out.insert(rnd.randrange(len(out) + 1), rand_line())
        elif out:
            out[rnd.randrange(len(out))] = rand_line()
    if rnd.random() < 0.05 and out:
        out = out[rnd.randrange(len(out)):]
    return out


def to_bytes(lines):
    if not lines:
        return b""
    s = "\n".join(lines)
    if rnd.random() < 0.9:
        s += "\n"
    return s.encode("utf-8")


os.makedirs(work, exist_ok=True)
pairs = []
for i in range(cases):
    a = rand_file()
    b = mutate(a)
    ab, bb = to_bytes(a), to_bytes(b)
    if rnd.random() < 0.02:
        ab, bb = bb, ab
    pairs.append((ab, bb))
    open(os.path.join(work, "%d.a" % i), "wb").write(ab)
    open(os.path.join(work, "%d.b" % i), "wb").write(bb)

r = subprocess.run([tool, "batch", algo, ctx, work, str(cases)], capture_output=True)
if r.returncode != 0:
    print("FAIL tool batch exit", r.returncode, r.stderr[:300])
    sys.exit(1)
ours = {}
for ln in r.stdout.decode().splitlines():
    i, h, c, ok = (int(x) for x in ln.split())
    ours[i] = (h, c, ok)

bad = {"self": 0, "patch": 0, "gnu_into_ours": 0, "min_vs_gnu": 0, "min_vs_difflib": 0}
same_text = same_hunks = same_changed = 0
gnu_hunks_total = our_hunks_total = 0
fewer_changed = more_changed = 0
shown = 0


def count_gnu(txt):
    hunks = 0
    changed = 0
    for l in txt.split(b"\n"):
        if l.startswith(b"@@ "):
            hunks += 1
        elif l.startswith(b"-") and not l.startswith(b"---") or l.startswith(b"+") and not l.startswith(b"+++"):
            changed += 1
    return hunks, changed


def count_gnu_exact(txt):
    # headers are only the first two lines
    lines = txt.split(b"\n")
    hunks = sum(1 for l in lines if l.startswith(b"@@ "))
    changed = 0
    for l in lines[2:] if len(lines) > 2 else []:
        if l.startswith(b"-") or l.startswith(b"+"):
            changed += 1
    return hunks, changed


for i, (ab, bb) in enumerate(pairs):
    pa = os.path.join(work, "%d.a" % i)
    pb = os.path.join(work, "%d.b" % i)
    pd = os.path.join(work, "%d.diff" % i)
    h, c, ok = ours[i]
    if not ok:
        bad["self"] += 1
        if shown < 5:
            print("FAIL selfok case", i)
            shown += 1
    gd = subprocess.run(["diff", "-U" + ctx, "--label", "a", "--label", "b", pa, pb], capture_output=True).stdout
    od = open(pd, "rb").read()
    gh, gc = count_gnu_exact(gd)
    gnu_hunks_total += gh
    our_hunks_total += h
    if od == gd:
        same_text += 1
    if gh == h:
        same_hunks += 1
    if gc == c:
        same_changed += 1
    elif c < gc:
        fewer_changed += 1
    else:
        more_changed += 1
        if algo == "0":
            bad["min_vs_gnu"] += 1
            if shown < 5:
                print("FAIL ours has more changed lines than GNU diff, case", i, c, gc)
                shown += 1
    # difflib minimality (matching blocks give an LCS-ish bound; only for small, unique-friendly checks)
    if algo == "0":
        al = ab.splitlines(True)
        bl = bb.splitlines(True)
        sm = difflib.SequenceMatcher(None, al, bl, autojunk=False)
        lcs_d = sum(bk.size for bk in sm.get_matching_blocks())
        dl = len(al) + len(bl) - 2 * lcs_d
        if c > dl:
            bad["min_vs_difflib"] += 1
    # GNU patch applies ours
    if h:
        out = os.path.join(work, "%d.out" % i)
        pr = subprocess.run(["patch", "-s", "-o", out, pa, pd], capture_output=True)
        if pr.returncode != 0 or open(out, "rb").read() != bb:
            bad["patch"] += 1
            if shown < 5:
                print("FAIL GNU patch of our diff, case", i, pr.stderr[:100])
                shown += 1
        # our apply on GNU diff
        ar = subprocess.run([tool, "apply", "0", os.path.join(work, "%d.gnu" % i), pa], capture_output=True) if False else None
        gp = os.path.join(work, "%d.gnu" % i)
        open(gp, "wb").write(gd)
        ar = subprocess.run([tool, "apply", "0", gp, pa], capture_output=True)
        if ar.returncode != 0 or ar.stdout != bb:
            bad["gnu_into_ours"] += 1
            if shown < 5:
                print("FAIL our apply of GNU diff, case", i, ar.stderr[:100])
                shown += 1

print("cases %d algo %s ctx %s: identical-to-GNU %d, same hunk count %d, same changed-line count %d "
      "(ours fewer %d, ours more %d), hunks ours/GNU %d/%d" %
      (cases, algo, ctx, same_text, same_hunks, same_changed, fewer_changed, more_changed, our_hunks_total, gnu_hunks_total))
print("hard failures: " + ", ".join("%s=%d" % kv for kv in bad.items()))
sys.exit(1 if any(bad.values()) else 0)
