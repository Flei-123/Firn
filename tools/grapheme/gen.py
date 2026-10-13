#!/usr/bin/env python3
"""tools/grapheme/gen.py -- generates generated/grapheme_tables.fi (UAX #29).

Inputs (Unicode 17.0.0, fixed by tools/grapheme/data/SHA256SUMS and
tools/ucd/UCD.sha256):
  tools/grapheme/data/GraphemeBreakProperty.txt   Grapheme_Cluster_Break
  tools/grapheme/data/emoji-data.txt              Extended_Pictographic
  tools/ucd/DerivedCoreProperties.txt             InCB (Consonant / Linker / Extend)

Output: a sorted list of ranges, one u64 each:
  bits 0..20   first code point
  bits 21..41  last code point
  bits 42..48  value: bits 0..3 GCB class, 4..5 InCB, 6 Extended_Pictographic
Code points that are not listed have value 0 (GCB Other, InCB None, no ExtPict).

Usage: python3 tools/grapheme/gen.py [--check]
"""
import hashlib
import os
import re
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
GBP = os.path.join(ROOT, "tools/grapheme/data/GraphemeBreakProperty.txt")
EMO = os.path.join(ROOT, "tools/grapheme/data/emoji-data.txt")
DCP = os.path.join(ROOT, "tools/ucd/DerivedCoreProperties.txt")
OUT = os.path.join(ROOT, "generated/grapheme_tables.fi")

CLASSES = ["Other", "CR", "LF", "Control", "Extend", "ZWJ", "Regional_Indicator", "Prepend",
           "SpacingMark", "L", "V", "T", "LV", "LVT"]
INCB = {"Consonant": 1, "Linker": 2, "Extend": 3}
MAXCP = 0x110000


def parse(path, prop_filter):
    out = []
    for line in open(path, encoding="utf-8"):
        line = line.split("#")[0].strip()
        if not line:
            continue
        rng, _, rest = line.partition(";")
        parts = [x.strip() for x in rest.split(";")]
        if not prop_filter(parts):
            continue
        r = rng.strip().split("..")
        lo = int(r[0], 16)
        hi = int(r[-1], 16)
        out.append((lo, hi, parts))
    return out


def build():
    val = [0] * MAXCP
    for lo, hi, p in parse(GBP, lambda p: True):
        c = CLASSES.index(p[0])
        for cp in range(lo, hi + 1):
            val[cp] |= c
    for lo, hi, p in parse(DCP, lambda p: len(p) == 2 and p[0] == "InCB"):
        for cp in range(lo, hi + 1):
            val[cp] |= INCB[p[1]] << 4
    for lo, hi, p in parse(EMO, lambda p: p[0] == "Extended_Pictographic"):
        for cp in range(lo, hi + 1):
            val[cp] |= 64
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
    o = []
    o.append("// SPDX-License-Identifier: MPL-2.0")
    o.append("// generated/grapheme_tables.fi -- GENERATED. Do not edit by hand.")
    o.append("//")
    o.append("// Arises with `python3 tools/grapheme/gen.py`; reads (Unicode 17.0.0):")
    for p in (GBP, EMO, DCP):
        o.append("//   %-24s sha256 %s" % (os.path.basename(p), sha(p)))
    o.append("//")
    o.append("// %d ranges of one u64 each, sorted: bits 0..20 first code point, 21..41 last," % n)
    o.append("// 42..48 value (bits 0..3 Grapheme_Cluster_Break, 4..5 InCB 1 Consonant /")
    o.append("// 2 Linker / 3 Extend, bit 6 Extended_Pictographic). Not listed = value 0.")
    o.append("// Read by lib/str/grapheme.fi.")
    o.append("export { RANGES, BYTES, load }")
    o.append("")
    o.append("#[no_gc]")
    o.append("fn w(p: *mut u8, i: usize, v: u64) {")
    o.append("    *((p as usize + i * 8) as *mut u64) = v")
    o.append("}")
    o.append("")
    o.append("const RANGES: usize = %d" % n)
    o.append("const BYTES: usize = %d" % (n * 8))
    o.append("")
    parts = 0
    for s in range(0, n, 256):
        o.append("#[no_gc]")
        o.append("fn part%d(p: *mut u8) {" % parts)
        for i in range(s, min(n, s + 256)):
            o.append("    w(p, %d, 0x%x)" % (i, words[i]))
        o.append("}")
        o.append("")
        parts += 1
    o.append("#[no_gc]")
    o.append("fn load(p: *mut u8) {")
    for i in range(parts):
        o.append("    part%d(p)" % i)
    o.append("}")
    return "\n".join(o) + "\n"


if __name__ == "__main__":
    text = render(build())
    if "--check" in sys.argv:
        same = os.path.exists(OUT) and open(OUT, encoding="utf-8").read() == text
        print("generated/grapheme_tables.fi %s" % ("up to date" if same else "STALE"))
        sys.exit(0 if same else 1)
    open(OUT, "w", encoding="utf-8").write(text)
    print("wrote %s (%d ranges)" % (OUT, text.count("    w(p, ")))
