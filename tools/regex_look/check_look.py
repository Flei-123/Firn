#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/regex_look/check_look.py <regex_probe> [count] -- lookaround in lib/regex
# against Python's re on random patterns (seeded, reproducible), r320.
#
# Same protocol and same comparison as tools/libmvp/check_regex.py (the probe
# is tools/libmvp/regex_probe.fi): the first match with all group offsets AND
# the replace-all result must agree. Every pattern here contains at least one
# lookaround; there are NO backreferences and NO capture groups INSIDE a
# lookaround (lib/regex refuses those, see its header).
#
# Python's `re` demands a FIXED width in a lookbehind, lib/regex accepts a
# bounded one. A variable-width lookbehind is therefore generated as a top-level
# alternation `(?<=X|YY)` and given to Python as `(?:(?<=X)|(?<=YY))` (negative:
# `(?<!X)(?<!YY)`), which means the same.
#
# Not generated (known, documented differences): a quantified group that can
# match the empty string; a quantified lookaround (refused by lib/regex).
import random, re, subprocess, sys

import os
BREAK = bool(os.environ.get("LOOK_BREAK"))   # counter-check: must make this script fail
probe = sys.argv[1]
count = int(sys.argv[2]) if len(sys.argv) > 2 else 6000


def py_translate(p, multiline=True):
    out, i = [], 0
    while i < len(p):
        c = p[i]
        if c == "\\" and i + 1 < len(p):
            out.append("\\Z" if p[i + 1] == "z" else p[i:i + 2]); i += 2; continue
        if c == "[":
            j = i + 1
            if j < len(p) and p[j] == "^": j += 1
            if j < len(p) and p[j] == "]": j += 1
            while j < len(p) and p[j] != "]":
                j += 2 if p[j] == "\\" else 1
            out.append(p[i:j + 1]); i = j + 1; continue
        out.append("\\Z" if c == "$" and not multiline else c); i += 1
    return "".join(out)


def python(p_py, flags, text):
    f = re.ASCII
    if flags & 1: f |= re.IGNORECASE
    if flags & 2: f |= re.MULTILINE
    if flags & 4: f |= re.DOTALL
    pp = py_translate(p_py, multiline=bool(flags & 2))
    try:
        r = re.compile(pp, f)
    except re.error:
        return "E"
    m = r.search(text)
    if not m:
        return "N"
    b = lambda s: len(text[:s].encode()) if s >= 0 else -1
    spans = []
    for g in range(r.groups + 1):
        s, e = m.span(g)
        spans += [b(s), b(e)]
    out, pos, last = [], 0, 0
    while pos <= len(text):
        mm = r.search(text, pos)
        if not mm:
            break
        out.append(text[last:mm.start()])
        out.append("<" + ((mm.group(1) or "") if r.groups >= 1 else "") + ">")
        last = mm.end()
        if mm.end() > mm.start():
            pos = mm.end()
        else:
            if mm.end() >= len(text):
                break
            out.append(text[mm.end()])
            last = pos = mm.end() + 1
    out.append(text[last:])
    return "M " + " ".join(map(str, spans)) + " | " + "".join(out).encode().hex()


# ---- generator. A pattern is a pair (firn_text, python_text).
SINGLE = ["a", "b", "c", "\xe4", "Ω", "x", ".", "\\d", "\\w", "\\s", "\\D", "\\W", "[ab]", "[^a]", "[a-cx]", "[\xe4\xf6]", "\\.", "-", "K"]
ZERO = ["\\b", "\\B", "^", "$"]


def fixed_body(rng, width):
    """a body of exactly `width` characters, no captures, as one string"""
    parts = []
    w = width
    while w > 0:
        r = rng.random()
        if r < 0.15 and w >= 2:
            k = rng.randint(2, min(w, 3))
            parts.append(rng.choice(SINGLE) + "{%d}" % k); w -= k
        elif r < 0.3 and w >= 2:
            k = rng.randint(1, min(w, 2))
            alts = ["".join(rng.choice(SINGLE) for _ in range(k)) for _ in range(rng.randint(2, 3))]
            parts.append("(?:" + "|".join(alts) + ")"); w -= k
        else:
            parts.append(rng.choice(SINGLE)); w -= 1
        if rng.random() < 0.1:
            parts.append(rng.choice(ZERO))
    return "".join(parts)


def free_body(rng, depth):
    """any body for a lookahead: quantifiers, alternation, non-capturing groups, nested lookaround"""
    parts = []
    for _ in range(rng.randint(1, 3)):
        r = rng.random()
        if r < 0.15 and depth < 2:
            atom = "(?:" + free_body(rng, depth + 1) + "|" + free_body(rng, depth + 1) + ")"
            can_be_empty = True
        elif r < 0.25 and depth < 2:
            atom = look(rng, depth + 1)[0]
            can_be_empty = True
        else:
            atom = rng.choice(SINGLE + ZERO)
            can_be_empty = atom in ZERO
        if not can_be_empty and rng.random() < 0.5:
            atom += rng.choice(["*", "+", "?", "{2}", "{1,3}", "{2,}", "*?", "+?"])
        parts.append(atom)
    return "".join(parts)


def look(rng, depth):
    """one lookaround: (firn, python)"""
    kind = rng.choice(["?=", "?!", "?<=", "?<!"])
    if kind in ("?=", "?!"):
        b = free_body(rng, depth)
        return "(" + kind + b + ")", "(" + kind + b + ")"
    if rng.random() < 0.3:
        # variable width: alternatives of different fixed widths
        alts = [fixed_body(rng, rng.randint(0, 3)) for _ in range(rng.randint(2, 3))]
        firn = "(" + kind + "|".join(alts) + ")"
        if kind == "?<=":
            py = "(?:" + "|".join("(?<=" + a + ")" for a in alts) + ")"
        else:
            py = "".join("(?<!" + a + ")" for a in alts)
        return firn, py
    b = fixed_body(rng, rng.randint(0, 3))
    if rng.random() < 0.15 and depth < 2:
        # a nested lookahead as part of a lookbehind body (width 0)
        inner = look(rng, depth + 1)[0]
        b = b + inner
    return "(" + kind + b + ")", "(" + kind + b + ")"


def rand_pattern(rng):
    firn, py = [], []
    n_look = 0
    for _ in range(rng.randint(1, 5)):
        r = rng.random()
        if r < 0.4:
            f, p = look(rng, 0)
            n_look += 1
        elif r < 0.5:
            inner = free_body(rng, 1)
            f = p = rng.choice(["(", "(?:"]) + inner + ")"
        else:
            f = p = rng.choice(SINGLE + ZERO)
            if f not in ZERO and rng.random() < 0.4:
                q = rng.choice(["*", "+", "?", "{2}", "{1,3}", "*?", "+?"])
                f += q; p += q
        firn.append(f); py.append(p)
    if n_look == 0:
        f, p = look(rng, 0)
        firn.insert(rng.randint(0, len(firn)), f); py.insert(len(py), p)
        # keep the same position in both
        py = list(firn)  # placeholder, rebuilt below
        return None
    return "".join(firn), "".join(py)


def rand_text(rng):
    alphabet = rng.choice(["abcx\xe4\xf6ΩK-. 12\n_", "ab", "abc x", "ab1-", "a\xe4b Ω"])
    return "".join(rng.choice(alphabet) for _ in range(rng.randint(0, 16)))


FIXED = [
    (r'["\']de["\']\s*[:=]|(?<![A-Za-z])de:|--lang=de\b', r'["\']de["\']\s*[:=]|(?<![A-Za-z])de:|--lang=de\b', 0, "x = 'de':  code:de: --lang=de"),
    (r"(?<![A-Za-z])de:", r"(?<![A-Za-z])de:", 0, "code:de: de:"),
    (r"[A-Z\xc4\xd6\xdc]+(?![a-z\xe4\xf6\xfc\xdf])", r"[A-Z\xc4\xd6\xdc]+(?![a-z\xe4\xf6\xfc\xdf])", 0, "ABc DEF \xc4\xd6\xfc \xdcB"),
    (r"[A-Z\xc4\xd6\xdc]?[a-z\xe4\xf6\xfc\xdf]+|[A-Z\xc4\xd6\xdc]+(?![a-z\xe4\xf6\xfc\xdf])", r"[A-Z\xc4\xd6\xdc]?[a-z\xe4\xf6\xfc\xdf]+|[A-Z\xc4\xd6\xdc]+(?![a-z\xe4\xf6\xfc\xdf])", 0, "Gro\xdfeSt\xe4dte HTTPServer \xc4\xd6\xdc"),
    (r"(a)(?=b)", r"(a)(?=b)", 0, "ab"), (r"(?<=a)(b)", r"(?<=a)(b)", 0, "ab"),
    (r"(?<=(a|b))c", None, 0, "ac"),  # capture inside: refused by lib/regex, Python accepts
]

rng = random.Random(20261008)
cases = [c for c in FIXED if c[1] is not None]
while len(cases) < len(FIXED) - 1 + count:
    r = rand_pattern(rng)
    if r is None:
        continue
    firn, py = r
    try:
        re.compile(py_translate(py))
    except re.error:
        continue
    cases.append((firn, py, rng.choice([0, 0, 0, 1, 2]), rand_text(rng)))

inp = "".join("%s %d %s\n" % (f.encode().hex(), fl, t.encode().hex()) for f, p, fl, t in cases)
inp += "%s 0 %s\n" % (FIXED[-1][0].encode().hex(), b"ac".hex())
out = subprocess.run([probe], input=inp.encode(), capture_output=True, check=True).stdout.decode().splitlines()
bad = 0
refused = 0
nlook = 0
matched = 0
for k, ((f, p, fl, t), got) in enumerate(zip(cases, out)):
    if BREAK:
        # counter-check: Python is asked the OPPOSITE question for every lookahead
        p = p.replace("(?=", "(?#T").replace("(?!", "(?=").replace("(?#T", "(?!")
    want = python(p, fl, t)
    if "(?=" in f or "(?!" in f or "(?<" in f:
        nlook += 1
    if got.startswith("E "):
        refused += 1
        if want == "E":
            continue
    if t == "" and "\\B" in f:
        continue  # Python's \B never matches an empty text; RE2's does
    if want.startswith("M"):
        matched += 1
    if got != want:
        bad += 1
        if bad <= 12:
            print("  DIFF %r (python %r) flags=%d text=%r\n     firn   %s\n     python %s" % (f, p, fl, t, got, want))
if out[-1] != "E Unsupported":
    bad += 1
    print("  a capture inside a lookaround must be refused as Unsupported, got %r" % out[-1])
if len(out) != len(cases) + 1:
    bad += 1
    print("  probe answered %d of %d lines" % (len(out), len(cases) + 1))
print("regex lookaround: %d cases (%d fixed, %d random), %d with a match, %d refused by both, %d differ"
      % (len(cases), len(FIXED) - 1, count, matched, refused, bad))
sys.exit(1 if bad else 0)
