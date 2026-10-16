#!/usr/bin/env python3
"""Cross-check of text.fuzzy against a brute-force reference.

usage: fuzzy_cross.py FUZZYTOOL CASES SEED

The reference enumerates EVERY placement of the pattern in the text (all increasing position tuples whose
characters match), scores each with the documented fzf rules (16 per match, boundary / camel / white / delimiter
bonus, first pattern character doubled, chunk bonus for consecutive matches, gap 3 + 1 per further skipped
character) and keeps the maximum. The tool must report the same best score, and its positions must be a valid
placement whose own score is that maximum. `rank` must return exactly the matching items, ordered by score
(descending), then character count (ascending), then input order.
Exit 1 when anything differs.
"""
import os
import random
import subprocess
import sys

tool, cases, seed = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
rnd = random.Random(seed)
# counter-check: with FUZZY_CROSS_SABOTAGE=1 the reference charges 2 per gap; the run has to report mismatches
GAP_START = 2 if os.environ.get("FUZZY_CROSS_SABOTAGE") else 3

ALPHA = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
PUNCT = " /,:;|_-.()[]{}  "
EXTRA = "äöüßéÉÀÿÇαβγΑΒΓдДжЖŁłŐő漢"
WHITE = set("   ")
DELIM = set("/,:;|")


def is_upper(ch):
    return ch.lower() != ch and len(ch.lower()) == 1


def is_lower(ch):
    return ch.islower()


def cls(ch):
    o = ord(ch)
    if o < 128:
        if ch.isalpha():
            return "U" if ch.isupper() else "L"
        if ch.isdigit():
            return "N"
        if ch == " " or 9 <= o <= 13:
            return "W"
        if ch in DELIM:
            return "D"
        return "X"
    if ch in WHITE:
        return "W"
    if is_upper(ch):
        return "U"
    if is_lower(ch):
        return "L"
    return "A"  # other letter


def bonus(prev, cur):
    if cur not in "WX":
        if prev == "W":
            return 10
        if prev == "D":
            return 9
        if prev == "X":
            return 8
    if (prev == "L" and cur == "U") or (prev != "N" and cur == "N"):
        return 7
    if cur in "XD":
        return 8
    if cur == "W":
        return 10
    return 0


def fold(ch):
    low = ch.lower()
    return low if len(low) == 1 else ch


def best(pattern, text, mode):
    has_upper = any(is_upper(c) for c in pattern)
    do_fold = mode == 1 or (mode == 0 and not has_upper)
    pat = [fold(c) if do_fold else c for c in pattern]
    txt = [fold(c) if do_fold else c for c in text]
    bon = []
    prev = "W"
    for c in text:
        k = cls(c)
        bon.append(bonus(prev, k))
        prev = k
    n = len(txt)
    result = [None]

    def score_of(tup):
        s = 0
        chunk = 0
        for k, pos in enumerate(tup):
            b = bon[pos]
            if k == 0:
                s += 16 + 2 * b
                chunk = b
            elif pos == tup[k - 1] + 1:
                if b >= 8 and b > chunk:
                    chunk = b
                    add = b
                else:
                    add = max(b, 4, chunk)
                s += 16 + add
            else:
                gap = pos - tup[k - 1] - 1
                s += 16 + b - (GAP_START + (gap - 1))
                chunk = b
        return s

    def rec(k, start, tup):
        if k == len(pat):
            sc = score_of(tup)
            if result[0] is None or sc > result[0]:
                result[0] = sc
            return
        for j in range(start, n - (len(pat) - k) + 1):
            if txt[j] == pat[k]:
                tup.append(j)
                rec(k + 1, j + 1, tup)
                tup.pop()

    rec(0, 0, [])
    return result[0], score_of, pat, txt


def rand_text():
    k = rnd.choice([1, 2, 3, 5, 8, 12, 16])
    pool = ALPHA * 2 + PUNCT * 2 + EXTRA * (1 if rnd.random() < 0.3 else 0)
    if rnd.random() < 0.4:
        pool = "ab" + "AB" + " /_"
    return "".join(rnd.choice(pool) for _ in range(k))


def rand_pattern(text):
    if rnd.random() < 0.6 and text:
        # a subsequence of the text, maybe damaged
        idx = sorted(rnd.sample(range(len(text)), min(len(text), rnd.randrange(1, 6))))
        p = "".join(text[i] for i in idx)
        p = "".join(c for c in p if c not in "\t\n")
        if rnd.random() < 0.3:
            p = p.swapcase()
        if rnd.random() < 0.1:
            p += rnd.choice(ALPHA)
        return p.strip("\t\n") or "a"
    return "".join(rnd.choice(ALPHA[:12] + ALPHA[26:30]) for _ in range(rnd.randrange(1, 4)))


lines = []
expect = []
for _ in range(cases):
    text = rand_text()
    pat = rand_pattern(text)
    mode = rnd.choice([0, 0, 1, 2])
    lines.append("M %d\t%s\t%s" % (mode, pat, text))
    expect.append(("M", pat, text, mode))
nrank = max(1, cases // 8)
for _ in range(nrank):
    items = [rand_text() for _ in range(rnd.randrange(1, 12))]
    pat = rand_pattern(rnd.choice(items))
    mode = rnd.choice([0, 1, 2])
    lines.append("R %d\t%s\t%s" % (mode, pat, "\t".join(items)))
    expect.append(("R", pat, items, mode))

r = subprocess.run([tool], input=("\n".join(lines) + "\n").encode("utf-8"), capture_output=True)
if r.returncode != 0:
    print("  FAIL  tool rc=%d %s" % (r.returncode, r.stderr[:200]))
    sys.exit(1)
out = r.stdout.decode("utf-8").split("\n")
bad = 0
nmatch = 0
shown = 0
for k, ex in enumerate(expect):
    got = out[k] if k < len(out) else ""
    if ex[0] == "M":
        _, pat, text, mode = ex
        sc, score_of, p, t = best(pat, text, mode)
        if sc is None:
            ok = got == "NO"
        else:
            nmatch += 1
            ok = False
            parts = got.split(" ")
            if len(parts) == 2 and parts[0] == str(sc):
                try:
                    pos = [int(x) for x in parts[1].split(",")]
                    ok = (len(pos) == len(p) and all(pos[i] < pos[i + 1] for i in range(len(pos) - 1))
                          and all(t[pos[i]] == p[i] for i in range(len(pos))) and score_of(pos) == sc)
                except (ValueError, IndexError):
                    ok = False
    else:
        _, pat, items, mode = ex
        scored = []
        for i, it in enumerate(items):
            sc, _, _, _ = best(pat, it, mode)
            if sc is not None:
                scored.append((-sc, len(it), i))
        scored.sort()
        want = " ".join(str(i) for _, _, i in scored)
        ok = got == want
    if not ok:
        bad += 1
        if shown < 5:
            shown += 1
            print("  DIFF  %r got %r" % (ex, got))
print("fuzzy_cross: %d match cases (%d matching) + %d rank cases, %d mismatches" % (cases, nmatch, nrank, bad))
sys.exit(1 if bad else 0)
