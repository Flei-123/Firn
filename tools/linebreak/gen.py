#!/usr/bin/env python3
"""tools/linebreak/gen.py -- generates generated/linebreak_tables.fi (UAX #14).

Inputs:
  tools/linebreak/data/LineBreak.txt        Line_Break (Unicode 15.0.0)
  tools/linebreak/data/EastAsianWidth.txt   East_Asian_Width (F, W, H matter for LB30)
  tools/grapheme/data/emoji-data.txt        Extended_Pictographic (LB30b)
  tools/ucd/UnicodeData.txt                 General_Category (LB1: SA -> CM for Mn/Mc), assigned code points

LB1 is applied here: AI, SG, XX -> AL; SA -> CM (Mn, Mc) or AL; CJ -> NS.
Output: sorted ranges, one u64 each: bits 0..20 first, 21..41 last, 42..49 value:
  bits 0..5 class (see CLASSES), bit 6 East_Asian_Width in F/W/H, bit 7 Extended_Pictographic and unassigned.
Not listed = AL (value 0).

Usage: python3 tools/linebreak/gen.py [--check]
"""
import hashlib, os, sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
LB = os.path.join(ROOT, "tools/linebreak/data/LineBreak.txt")
EAW = os.path.join(ROOT, "tools/linebreak/data/EastAsianWidth.txt")
EMO = os.path.join(ROOT, "tools/grapheme/data/emoji-data.txt")
UD = os.path.join(ROOT, "tools/ucd/UnicodeData.txt")
OUT = os.path.join(ROOT, "generated/linebreak_tables.fi")
CLASSES = ["AL", "BA", "BB", "BK", "CB", "CL", "CM", "CP", "CR", "EB", "EM", "EX", "GL", "H2", "H3",
           "HL", "HY", "ID", "IN", "IS", "JL", "JT", "JV", "LF", "NL", "NS", "NU", "OP", "PO", "PR",
           "QU", "RI", "SP", "SY", "WJ", "ZW", "ZWJ", "B2"]
MAXCP = 0x110000


def parse(path):
    for line in open(path, encoding="utf-8"):
        line = line.split("#")[0].strip()
        if not line:
            continue
        rng, _, rest = line.partition(";")
        r = rng.strip().split("..")
        yield int(r[0], 16), int(r[-1], 16), rest.strip().split()[0]


def build():
    cls = ["XX"] * MAXCP
    for lo, hi, c in parse(LB):
        for cp in range(lo, hi + 1):
            cls[cp] = c
    gc = {}
    assigned = bytearray(MAXCP)
    first = None
    for line in open(UD, encoding="utf-8"):
        f = line.split(";")
        cp = int(f[0], 16)
        if f[1].endswith(", First>"):
            first = cp
            continue
        if f[1].endswith(", Last>"):
            for c in range(first, cp + 1):
                assigned[c] = 1
                gc[c] = f[2]
            continue
        assigned[cp] = 1
        gc[cp] = f[2]
    fwh = bytearray(MAXCP)
    for lo, hi, c in parse(EAW):
        if c in ("F", "W", "H"):
            for cp in range(lo, hi + 1):
                fwh[cp] = 1
    ext = bytearray(MAXCP)
    for lo, hi, c in parse(EMO):
        if c == "Extended_Pictographic":
            for cp in range(lo, hi + 1):
                ext[cp] = 1
    val = [0] * MAXCP
    for cp in range(MAXCP):
        c = cls[cp]
        if c in ("AI", "SG", "XX"):
            c = "AL"
        elif c == "SA":
            c = "CM" if gc.get(cp) in ("Mn", "Mc") else "AL"
        elif c == "CJ":
            c = "NS"
        v = CLASSES.index(c)
        if fwh[cp]:
            v |= 64
        if ext[cp] and not assigned[cp]:
            v |= 128
        val[cp] = v
    ranges = []
    cp = 0
    while cp < MAXCP:
        v = val[cp]
        if v == 0:
            cp += 1
            continue
        e = cp
        while e + 1 < MAXCP and val[e + 1] == v:
            e += 1
        ranges.append((cp, e, v))
        cp = e + 1
    return ranges


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def render(ranges):
    words = [lo | (hi << 21) | (v << 42) for lo, hi, v in ranges]
    n = len(words)
    o = ["// SPDX-License-Identifier: MPL-2.0",
         "// generated/linebreak_tables.fi -- GENERATED. Do not edit by hand.", "//",
         "// Arises with `python3 tools/linebreak/gen.py`; reads:"]
    for p in (LB, EAW, EMO, UD):
        o.append("//   %-22s sha256 %s" % (os.path.basename(p), sha(p)))
    o += ["//", "// %d ranges of one u64 each, sorted: bits 0..20 first code point, 21..41 last," % n,
          "// 42..49 value (bits 0..5 line break class after LB1, bit 6 East_Asian_Width F/W/H,",
          "// bit 7 Extended_Pictographic and unassigned). Not listed = AL (0).",
          "// Read by lib/str/linebreak.fi.",
          "export { RANGES, BYTES, load }", "", "#[no_gc]",
          "fn w(p: *mut u8, i: usize, v: u64) {",
          "    *((p as usize + i * 8) as *mut u64) = v", "}", "",
          "const RANGES: usize = %d" % n, "const BYTES: usize = %d" % (n * 8), ""]
    parts = 0
    for s in range(0, n, 256):
        o += ["#[no_gc]", "fn part%d(p: *mut u8) {" % parts]
        for i in range(s, min(n, s + 256)):
            o.append("    w(p, %d, 0x%x)" % (i, words[i]))
        o += ["}", ""]
        parts += 1
    o += ["#[no_gc]", "fn load(p: *mut u8) {"]
    for i in range(parts):
        o.append("    part%d(p)" % i)
    o.append("}")
    return "\n".join(o) + "\n"


if __name__ == "__main__":
    text = render(build())
    if "--check" in sys.argv:
        same = os.path.exists(OUT) and open(OUT, encoding="utf-8").read() == text
        print("generated/linebreak_tables.fi %s" % ("up to date" if same else "STALE"))
        sys.exit(0 if same else 1)
    open(OUT, "w", encoding="utf-8").write(text)
    print("wrote %s (%d ranges)" % (OUT, text.count("    w(p, ")))
