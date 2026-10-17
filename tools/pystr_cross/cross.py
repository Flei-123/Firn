#!/usr/bin/env python3
"""Cross-check std.pystr and regex.pyre against Python str / re.

usage: cross.py TOOL WORKDIR SEED CASES
  * every str method of std.pystr on random text (ASCII, whitespace zoo, Latin-1,
    CJK, emoji, U+0085/U+00A0/U+2003/U+3000/U+2028), needles taken from the
    text, empty needles, maxsplit/count variants; ASCII-only text for the case
    and class methods (documented ASCII-only); find/rfind compare OCTET offsets.
  * regex.pyre findall/sub/subn/split/search/escape on random patterns (no
    pattern that can match empty, except the `x*` family, where Python 3.7+
    agrees) over ASCII text.
  * fuzz: random octets (invalid UTF-8 included) through every op: the tool
    must exit 0 and answer one line per op (no trap).
Exit 0 and prints the mismatch count (target 0). The reference-quirk list is in
tools/pystr_cross/NOTES.md.
"""
import os
import random
import re
import subprocess
import sys

tool, work, seed, cases = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
rng = random.Random(seed)
os.makedirs(work, exist_ok=True)
bad = 0
checked = 0

ASCII = list("abcdeABCDE012 ,;.-_xyzXYZ\t\n\r") + ["ab", "AB", "  ", "a1", "A-b"]
WIDE = ASCII + ["ä", "ß", "€", "\U0001f600", "日本", "\xa0", " ", "　", "\x85", " ", "\x1c", "\x0b", "\x0c", "é", "Ω"]
SPACE_ZOO = [" ", "\t", "\n", "\r", "\x0b", "\x0c", "\x1c", "\x1f", "\x85", "\xa0", " ", " ", " ", " ", " ", " ", " ", "　"]


def rtext(alpha, maxn=14):
    return "".join(rng.choice(alpha) for _ in range(rng.randrange(0, maxn)))


def needle(s, alpha):
    k = rng.randrange(6)
    if k == 0:
        return ""
    if k < 4 and s:
        a = rng.randrange(len(s))
        return s[a:a + rng.randrange(1, 4)]
    return rtext(alpha, 4) or "a"


def hx(s):
    return s.encode("utf-8").hex()


def lst(v):
    return "%d:%s" % (len(v), ",".join(hx(x) for x in v))


def run_ops(ops, exps, what):
    """ops: list of tab-joined lines; exps: expected output text (one per op, a
    string with embedded newlines for ops that print several lines)."""
    global bad, checked
    path = os.path.join(work, "ops.txt")
    with open(path, "w") as f:
        f.write("\n".join(ops) + "\n")
    p = subprocess.run([tool, path], capture_output=True, timeout=300)
    got = p.stdout.decode("utf-8", "replace").split("\n")
    if got and got[-1] == "":
        got.pop()
    want = "\n".join(exps).split("\n") if exps else []
    checked += len(ops)
    if p.returncode != 0:
        print("FAIL tool exit", p.returncode, what)
        bad += 1
        return
    if len(got) != len(want):
        print("FAIL line count", len(got), len(want), what)
        bad += 1
        return
    # map back to ops for the report
    gi = 0
    for op, e in zip(ops, exps):
        n = e.count("\n") + 1
        g = "\n".join(got[gi:gi + n])
        gi += n
        if g != e:
            bad += 1
            if bad <= 12:
                print("MISMATCH", what, repr(op[:100]), "want", e[:80], "got", g[:80])


def str_ops():
    ops, exps = [], []
    for _ in range(cases):
        s = rtext(WIDE)
        a = rtext(ASCII)
        sep = needle(s, WIDE) or ","
        # case methods: ASCII text
        for name, fn in (("lower", str.lower), ("upper", str.upper), ("capitalize", str.capitalize),
                         ("title", str.title), ("swapcase", str.swapcase)):
            ops.append("%s\t%s" % (name, hx(a)))
            exps.append(hx(fn(a)))
        for name, fn in (("isdigit", str.isdigit), ("isalpha", str.isalpha), ("isalnum", str.isalnum),
                         ("isupper", str.isupper), ("islower", str.islower)):
            t = rtext(list("abAB01 _-") + ["x", "Z", "9"], 5)
            ops.append("%s\t%s" % (name, hx(t)))
            exps.append("T" if fn(t) else "F")
        z = rtext(SPACE_ZOO + ["a"], 5)
        ops.append("isspace\t%s" % hx(z))
        exps.append("T" if z.isspace() else "F")
        ops.append("isascii\t%s" % hx(s))
        exps.append("T" if s.isascii() else "F")
        ops.append("charlen\t%s" % hx(s))
        exps.append(str(len(s)))
        # strip family (whitespace zoo around words)
        w = rtext(SPACE_ZOO, 4) + rtext(WIDE, 6) + rtext(SPACE_ZOO, 4)
        for name, fn in (("strip", str.strip), ("lstrip", str.lstrip), ("rstrip", str.rstrip)):
            ops.append("%s\t%s" % (name, hx(w)))
            exps.append(hx(fn(w)))
        chars = rtext(WIDE, 4)
        for name, fn in (("strip_chars", str.strip), ("lstrip_chars", str.lstrip), ("rstrip_chars", str.rstrip)):
            ops.append("%s\t%s\t%s" % (name, hx(s), hx(chars)))
            exps.append(hx(fn(s, chars)))
        # split family
        ops.append("split\t%s\t%s" % (hx(s), hx(sep)))
        exps.append(lst(s.split(sep)))
        mx = rng.choice([-1, -1, 0, 1, 2, 3])
        ops.append("split_n\t%s\t%s\t%d" % (hx(s), hx(sep), mx))
        exps.append(lst(s.split(sep, mx)))
        t = rtext(WIDE + SPACE_ZOO, 14)
        ops.append("split_ws\t%s" % hx(t))
        exps.append(lst(t.split()))
        ops.append("splitlines\t%s" % hx(t))
        exps.append(lst(t.splitlines()))
        items = [rtext(WIDE, 4) for _ in range(rng.randrange(0, 5))]
        ops.append("join\t%s\t%s" % (lst(items), hx(rtext(WIDE, 3))))
        sp = rtext(WIDE, 3)
        exps.append(hx(sp.join(items)))
        # fix: the sep must be the one used above
        ops[-1] = "join\t%s\t%s" % (lst(items), hx(sp))
        items2 = [rng.choice(["a", "b", "", "ä", "ab", "B", "日"]) for _ in range(rng.randrange(0, 8))]
        ops.append("sorted_set\t%s" % lst(items2))
        exps.append(lst(sorted(set(items2))))
        # replace
        old, new = needle(s, WIDE), rtext(WIDE, 3)
        ops.append("replace\t%s\t%s\t%s" % (hx(s), hx(old), hx(new)))
        exps.append(hx(s.replace(old, new)))
        cnt = rng.choice([-1, 0, 1, 2, 5])
        ops.append("replace_n\t%s\t%s\t%s\t%d" % (hx(s), hx(old), hx(new), cnt))
        exps.append(hx(s.replace(old, new, cnt)))
        n = rng.randrange(0, 4)
        ops.append("repeat\t%s\t%d" % (hx(s), n))
        exps.append(hx(s * n))
        # search family
        nd = needle(s, WIDE)
        ops.append("startswith\t%s\t%s" % (hx(s), hx(nd)))
        exps.append("T" if s.startswith(nd) else "F")
        ops.append("endswith\t%s\t%s" % (hx(s), hx(nd)))
        exps.append("T" if s.endswith(nd) else "F")
        ops.append("contains\t%s\t%s" % (hx(s), hx(nd)))
        exps.append("T" if nd in s else "F")
        ops.append("count\t%s\t%s" % (hx(s), hx(nd)))
        exps.append(str(s.count(nd)))
        sb, nb = s.encode(), nd.encode()
        ops.append("find\t%s\t%s" % (hx(s), hx(nd)))
        exps.append(str(sb.find(nb)))
        ops.append("rfind\t%s\t%s" % (hx(s), hx(nd)))
        exps.append(str(sb.rfind(nb)))
        # justify
        w2 = rng.randrange(0, 20)
        fill = rng.choice([32, 48, 42, 45])
        for name, fn in (("ljust", str.ljust), ("rjust", str.rjust), ("center", str.center)):
            ops.append("%s\t%s\t%d\t%d" % (name, hx(s), w2, fill))
            exps.append(hx(fn(s, w2, chr(fill))))
        zs = rng.choice(["", "-", "+", "12", "-12", "+7", "abc", "-x", "ä"]) 
        ops.append("zfill\t%s\t%d" % (hx(zs), w2))
        exps.append(hx(zs.zfill(w2)))
    for i in range(0, len(ops), 4000):
        run_ops(ops[i:i + 4000], exps[i:i + 4000], "str")


# --------------------------------------------------------------- regex
def rpat():
    """A random pattern without `|`-empty tricks, matching at least one char.
    Returns (pattern, ngroups, group_always_participates)."""
    atoms = ["a", "b", "c", "x", r"\d", r"\w", ".", "[a-c]", "[^a ]", r"\s", "ab", "-", r"\.", "[0-9]", "A"]
    quants = ["", "", "", "+", "*", "?", "{2}", "{1,3}", "+?", "*?"]
    parts = []
    k = rng.randrange(1, 4)
    for j in range(k):
        a = rng.choice(atoms)
        q = rng.choice(quants if j else ["", "", "+", "{2}", "{1,3}", "+?"])
        parts.append(a + q)
    pat = "".join(parts)
    ngroups = 0
    if rng.randrange(3) == 0:
        alt = rng.choice(atoms) + rng.choice(["", "+"])
        pat = "(?:%s|%s)" % (pat, alt) + rng.choice(["", "x", r"\d"])
    if rng.randrange(3) == 0:
        # one mandatory capture group around a prefix
        pat = "(" + pat + ")" + rng.choice(["", "-", "[a-c]*"])
        ngroups = 1
    return pat, ngroups


def rtext_re():
    words = ["abc", "a-b", "x1", "12", "A", "b", "c.d", "aab", "xx", "9"]
    return "".join(rng.choice(words + [" ", "\n", "_", "-", ""]) for _ in range(rng.randrange(0, 9)))


def regex_ops():
    ops, exps = [], []
    for _ in range(cases):
        pat, ng = rpat()
        text = rtext_re()
        try:
            rx = re.compile(pat)
        except re.error:
            continue
        # findall
        fa = rx.findall(text)
        ops.append("findall\t%s\t%s" % (hx(pat), hx(text)))
        exps.append(lst(fa))
        # sub
        repl = rng.choice(["X", "<\\g<0>>", "", "-\\\\-", "a\\nb", "$1", "[\\1]" if ng else "[\\g<0>]"])
        ops.append("sub\t%s\t%s\t%s" % (hx(pat), hx(repl), hx(text)))
        exps.append(hx(rx.sub(repl, text)))
        sn = rx.subn(repl, text)
        ops.append("subn\t%s\t%s\t%s" % (hx(pat), hx(repl), hx(text)))
        exps.append("%d\n%s" % (sn[1], hx(sn[0])))
        # split
        sp = rx.split(text)
        if None not in sp:
            ops.append("resplit\t%s\t%s" % (hx(pat), hx(text)))
            exps.append(lst(sp))
        m = rx.search(text)
        ops.append("span\t%s\t%s" % (hx(pat), hx(text)))
        exps.append("%d,%d" % m.span() if m else "-1")
        e = rtext(WIDE, 8)
        ops.append("escape\t%s" % hx(e))
        exps.append(hx(re.escape(e)))
    # the x* family: Python 3.7+ agrees on greedy stars
    for _ in range(cases // 4 + 1):
        pat = rng.choice(["a*", "x*", r"\d*", "[ab]*", "b?"])
        text = rtext_re()
        rx = re.compile(pat)
        ops.append("findall\t%s\t%s" % (hx(pat), hx(text)))
        exps.append(lst(rx.findall(text)))
        sn = rx.subn("-", text)
        ops.append("subn\t%s\t%s\t%s" % (hx(pat), hx("-"), hx(text)))
        exps.append("%d\n%s" % (sn[1], hx(sn[0])))
    for i in range(0, len(ops), 4000):
        run_ops(ops[i:i + 4000], exps[i:i + 4000], "regex")


def fuzz():
    """Random octets through every string op: no crash, one answer line per op."""
    global bad, checked
    names1 = ["lower", "upper", "capitalize", "title", "swapcase", "strip", "lstrip", "rstrip", "split_ws",
              "splitlines", "isdigit", "isalpha", "isalnum", "isspace", "isupper", "islower", "isascii", "charlen",
              "escape"]
    names2 = ["strip_chars", "lstrip_chars", "rstrip_chars", "split", "startswith", "endswith", "contains", "count",
              "find", "rfind"]
    ops = []
    for _ in range(cases * 4):
        a = bytes(rng.randrange(256) for _ in range(rng.randrange(0, 12)))
        if rng.randrange(2):
            a = bytes(rng.choice([0x80, 0xC3, 0xE2, 0xF0, 0xBF, 0x41, 0x20, 0xC0, 0xED, 0xF4, 0xFF]) for _ in range(rng.randrange(0, 12)))
        b = bytes(rng.randrange(256) for _ in range(rng.randrange(1, 4)))
        k = rng.randrange(4)
        if k == 0:
            ops.append("%s\t%s" % (rng.choice(names1), a.hex()))
        elif k == 1:
            ops.append("%s\t%s\t%s" % (rng.choice(names2), a.hex(), b.hex()))
        elif k == 2:
            ops.append("replace\t%s\t%s\t%s" % (a.hex(), b.hex(), bytes(rng.randrange(256) for _ in range(3)).hex()))
        else:
            ops.append("%s\t%s\t%d\t%d" % (rng.choice(["ljust", "rjust", "center"]), a.hex(), rng.randrange(0, 20), 32))
        ops.append("findall\t%s\t%s" % (hx(r"\w+|\W"), a.hex()))
        ops.append("sub\t%s\t%s\t%s" % (hx(r"(.)"), hx(r"<\1>"), a.hex()))
        ops.append("resplit\t%s\t%s" % (hx(r"[,;]+"), a.hex()))
    p = os.path.join(work, "fuzz.txt")
    with open(p, "w") as f:
        f.write("\n".join(ops) + "\n")
    r = subprocess.run([tool, p], capture_output=True, timeout=300)
    lines = r.stdout.split(b"\n")
    nl = len(lines) - 1
    checked += len(ops)
    if r.returncode != 0 or nl < len(ops):
        print("FAIL fuzz exit", r.returncode, "lines", nl, "ops", len(ops))
        bad += 1
    if b"?unknown" in r.stdout:
        print("FAIL fuzz unknown op")
        bad += 1


str_ops()
regex_ops()
fuzz()
print("pystr_cross: checked %d ops, mismatches %d" % (checked, bad))
sys.exit(1 if bad else 0)
