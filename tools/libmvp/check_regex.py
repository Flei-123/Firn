#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_regex.py <regex_probe> -- lib/regex against Python's re
# on a fixed corpus plus random patterns (seeded, reproducible).
#
# Python's re backtracks and lib/regex does not; their answers are
# nevertheless the same leftmost-first answers for every pattern this
# generator makes -- with one known class of exceptions, left out on
# purpose: a quantified group that can match the empty string ((a*)*,
# (a?)+ ...). There Perl/Python let the group take part once with an empty
# match, RE2 and this library do not enter the loop again (RE2's documented
# behaviour). The generator does not quantify groups whose content can be
# empty.
import random, re, subprocess, sys
probe = sys.argv[1]
count = int(sys.argv[2]) if len(sys.argv) > 2 else 3000

def py_translate(p, multiline=True):
    # \z is spelled \Z in Python; $ without (?m) means the very end in RE2
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

def python(p, flags, text, unicode_fold=False):
    # \d \w \s \b are ASCII in lib/regex (as in RE2): re.ASCII -- except
    # for the fixed cases that test Unicode case folding and use none of them
    f = 0 if unicode_fold else re.ASCII
    if flags & 1: f |= re.IGNORECASE
    if flags & 2: f |= re.MULTILINE
    if flags & 4: f |= re.DOTALL
    # (?m) inline is not generated; the flag decides what $ means
    pp = py_translate(p, multiline=bool(flags & 2))
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
    # replace-all with RE2's iteration (Python's re.sub differs after an
    # empty match: it may match again, non-empty, at the same position;
    # RE2, Rust and lib/regex step over one character instead)
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
    replaced = "".join(out)
    return "M " + " ".join(map(str, spans)) + " | " + replaced.encode().hex()

FIXED = [
    (r"-K(\d+)", 1, "-Q1 -k12"), (r"a|ab", 0, "ab"), (r"ab|a", 0, "ab"), (r"a*?b", 0, "aaab"),
    (r"(a+)(b+)?", 0, "aaac"), (r"^\s*(\w+)\s*=\s*(.*?)\s*$", 2, "  x = 1 \nkey =  value  "),
    (r"\bK\d\b", 0, "AK1 K2 K3x"), (r"\BK", 0, "K AK"), (r"(?i)schütz", 0, "SCHÜTZ Schütz"),
    (r"[äöü]+", 0, "Häuser"), (r"[^a-z]+", 0, "abc123def"), (r"x{2,3}", 0, "xxxxx"),
    (r"x{2,3}?", 0, "xxxxx"), (r"x{2,}", 0, "x xx xxxxx"), (r"(\d{4})-(\d{2})-(\d{2})", 0, "on 2026-09-25."),
    (r"(?P<tag>-[A-Z]+)(?P<num>\d+)", 0, "=+A1-QA12"), (r".", 4, "\n"), (r".", 0, "\nx"),
    (r"a.c", 0, "abc a\nc"), (r"(?s)a.c", 0, "a\nc"), (r"\x41\x{42}", 0, "AB"), (r"[\]\-]+", 0, "a]-]b"),
    (r"[a\-z]+", 0, "q-a-z"), (r"\.\*\+\?\(\)\[\]\{\}\|\^\$", 0, ".*+?()[]{}|^$"), (r"a{", 0, "a{"),
    (r"(a)|(b)", 0, "b"), (r"(?:ab)+", 0, "ababab"), (r"(ab)+", 0, "ababab"), (r"ΩΣ", 1, "ωσ ως"),
    (r"(?i)ж", 0, "Ж"), (r"\d+", 0, "abc"), (r"", 0, "abc"), (r"x*", 0, "abc"), (r"a*", 0, "baaac"),
    (r"(a)(?=b)", 0, "ab"), (r"(a)\1", 0, "aa"), (r"(", 0, ""), (r")", 0, ""), (r"[a", 0, ""),
    (r"*a", 0, ""), (r"a**", 0, "aaa"), (r"x{3,2}", 0, ""), (r"\q", 0, ""),
]

def rand_pattern(rng, depth=0):
    atoms = ["a", "b", "c", "ä", "Ω", "x", ".", "\\d", "\\w", "\\s", "\\D", "\\W", "[ab]", "[^a]", "[a-cx]",
             "[äö]", "\\.", "-", "\\b", "\\B", "^", "$", "K"]
    parts = []
    for _ in range(rng.randint(1, 4)):
        r = rng.random()
        if r < 0.15 and depth < 2:
            inner = rand_pattern(rng, depth + 1)
            kind = rng.choice(["(", "(?:", "(?P<g%d>" % rng.randint(0, 99)])
            atom = kind + inner + ")"
            can_be_empty = re.fullmatch(re.compile(py_translate(inner).replace("$", "\\Z")), "") is not None \
                if all(ch not in inner for ch in "^$\\b") else True
        elif r < 0.25 and depth < 2:
            atom = "(" + rand_pattern(rng, depth + 1) + "|" + rand_pattern(rng, depth + 1) + ")"
            can_be_empty = True
        else:
            atom = rng.choice(atoms)
            can_be_empty = atom in ("\\b", "\\B", "^", "$")
        if not can_be_empty and rng.random() < 0.5:
            atom += rng.choice(["*", "+", "?", "{2}", "{1,3}", "{2,}", "*?", "+?", "??", "{1,2}?"])
        parts.append(atom)
    return "".join(parts)

def rand_text(rng):
    alphabet = "abcxäöΩK-. 12\n_"
    return "".join(rng.choice(alphabet) for _ in range(rng.randint(0, 14)))

def text_is_empty_nb(p, t):
    return t == "" and "\\B" in p

rng = random.Random(20260925)
cases = list(FIXED)
while len(cases) < len(FIXED) + count:
    p = rand_pattern(rng)
    try:
        re.compile(py_translate(p))
    except re.error:
        continue
    cases.append((p, rng.choice([0, 0, 0, 1, 2]), rand_text(rng)))

inp = "".join("%s %d %s\n" % (p.encode().hex(), f, t.encode().hex()) for p, f, t in cases)
out = subprocess.run([probe], input=inp.encode(), capture_output=True, check=True).stdout.decode().splitlines()
bad = 0
refused = 0
for k, ((p, f, t), got) in enumerate(zip(cases, out)):
    want = python(p, f, t, unicode_fold=k < len(FIXED) and (f & 1 or "(?i" in p))
    if got.startswith("E "):
        refused += 1
        if want == "E" or "Unsupported" in got:
            continue
    if text_is_empty_nb(p, t):
        continue  # Python's \B never matches an empty text; RE2's does
    if want == "E" and "\\x{" in p:
        continue  # Python has no \x{...}; lib/regex has
    if got != want:
        bad += 1
        if bad <= 12:
            print("  DIFF %r flags=%d text=%r\n     firn   %s\n     python %s" % (p, f, t, got, want))
if len(out) != len(cases):
    bad += 1
    print("  probe answered %d of %d lines" % (len(out), len(cases)))
print("regex: %d cases (%d fixed, %d random), %d refused as expected, %d differ" % (len(cases), len(FIXED), count, refused, bad))
sys.exit(1 if bad else 0)
