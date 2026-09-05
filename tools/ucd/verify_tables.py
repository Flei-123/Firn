#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/ucd/verify_tables.py -- holds the GENERATED table against a parser
of its own, over ALL 1,114,112 code points.

Input: the standard output of `tools/ucd/probe_tables`, which asks
`generated/unicode_tables.fi` about every code point:

    C <lo> <hi> <class>    a run with the same class octet
    U <cp> <target>        an upper case mapping that differs
    L <cp> <target>        a lower case mapping that differs

This script reads `UnicodeData.txt` and `DerivedCoreProperties.txt` again,
entirely independently of the `comptime` blocks and of `pack.fi` -- in
Python, with Python's own splitting of the fields -- and then compares, code
point by code point, not by sampling:

  * the general category of every code point (bits 0..4 of the class octet),
  * ID_Start (bit 5) and ID_Continue (bit 6),
  * every simple upper and lower case mapping, in both directions: none
    missing, none invented.

Usage:  python3 tools/ucd/verify_tables.py <answers> [ucd-dir]
Exit code 0 = identical, 1 = a difference (with the first ten printed).
"""
import os
import sys

MAX_CP = 0x110000
CATS = ["Cn", "Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Me", "Nd", "Nl",
        "No", "Pc", "Pd", "Ps", "Pe", "Pi", "Pf", "Po", "Sm", "Sc", "Sk",
        "So", "Zs", "Zl", "Zp", "Cc", "Cf", "Cs", "Co"]
CAT_ID = {c: i for i, c in enumerate(CATS)}


def reference(here):
    """The expectation, straight out of the two UCD files."""
    cat = bytearray(MAX_CP)
    up, low = {}, {}
    pending = None
    lines = 0
    with open(os.path.join(here, "UnicodeData.txt"), "rb") as f:
        for raw in f:
            fields = raw.rstrip(b"\n").split(b";")
            if len(fields) < 15:
                continue
            lines += 1
            cp = int(fields[0], 16)
            name = fields[1]
            c = fields[2].decode("ascii")
            if c not in CAT_ID:
                raise SystemExit("unknown category %r at U+%04X" % (c, cp))
            if name.endswith(b", First>"):
                pending = (cp, c)
                continue
            if name.endswith(b", Last>"):
                if pending is None:
                    raise SystemExit("a `Last>` without a `First>` at %X" % cp)
                lo, lcat = pending
                if lcat != c:
                    raise SystemExit("First/Last with different categories at %X" % cp)
                for x in range(lo, cp + 1):
                    cat[x] = CAT_ID[c]
                pending = None
                continue
            cat[cp] = CAT_ID[c]
            if fields[12]:
                up[cp] = int(fields[12], 16)
            if fields[13]:
                low[cp] = int(fields[13], 16)
    if pending is not None:
        raise SystemExit("a `First>` without a `Last>`")
    ids = bytearray(MAX_CP)
    idc = bytearray(MAX_CP)
    props = 0
    with open(os.path.join(here, "DerivedCoreProperties.txt"), encoding="utf-8") as f:
        for raw in f:
            body = raw.split("#")[0].strip()
            if not body:
                continue
            parts = [p.strip() for p in body.split(";")]
            if len(parts) < 2 or parts[1] not in ("ID_Start", "ID_Continue"):
                continue
            props += 1
            r = parts[0]
            if ".." in r:
                a, b = r.split("..")
            else:
                a, b = r, r
            target = ids if parts[1] == "ID_Start" else idc
            for x in range(int(a, 16), int(b, 16) + 1):
                target[x] = 1
    cls = bytearray(MAX_CP)
    for x in range(MAX_CP):
        cls[x] = cat[x] | (ids[x] << 5) | (idc[x] << 6)
    return cls, up, low, lines, props


def answers(path):
    cls = bytearray(MAX_CP)
    up, low = {}, {}
    runs = 0
    seen = 0
    with open(path, encoding="ascii") as f:
        for nr, line in enumerate(f, 1):
            line = line.rstrip("\n")
            if not line:
                continue
            parts = line.split(" ")
            tag = parts[0]
            if tag == "C":
                if len(parts) != 4:
                    raise SystemExit("line %d: %r" % (nr, line))
                lo, hi, k = int(parts[1]), int(parts[2]), int(parts[3])
                if lo != seen:
                    raise SystemExit("line %d: the runs have a hole or overlap "
                                     "(%d after %d)" % (nr, lo, seen))
                if hi < lo or hi >= MAX_CP:
                    raise SystemExit("line %d: %d..%d" % (nr, lo, hi))
                if k > 127:
                    raise SystemExit("line %d: class %d has bits nobody set" % (nr, k))
                for x in range(lo, hi + 1):
                    cls[x] = k
                seen = hi + 1
                runs += 1
            elif tag == "U":
                up[int(parts[1])] = int(parts[2])
            elif tag == "L":
                low[int(parts[1])] = int(parts[2])
            else:
                raise SystemExit("line %d: unknown tag %r" % (nr, tag))
    if seen != MAX_CP:
        raise SystemExit("the runs stop at %d, not at %d" % (seen, MAX_CP))
    return cls, up, low, runs


def compare_maps(name, ref, got, out):
    wrong = []
    for cp in sorted(set(ref) | set(got)):
        if ref.get(cp) != got.get(cp):
            wrong.append((cp, ref.get(cp), got.get(cp)))
    if wrong:
        out.append("   %s DIFFERENT   %d code points, the first ten:" % (name, len(wrong)))
        for cp, a, b in wrong[:10]:
            out.append("     U+%04X  UCD %s  table %s"
                       % (cp, "-" if a is None else "U+%04X" % a,
                          "-" if b is None else "U+%04X" % b))
    return len(wrong)


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    here = sys.argv[2] if len(sys.argv) > 2 else os.path.dirname(os.path.abspath(__file__))
    ref_cls, ref_up, ref_low, lines, props = reference(here)
    got_cls, got_up, got_low, runs = answers(sys.argv[1])

    print("   UnicodeData.txt    %d lines" % lines)
    print("   DerivedCoreProperties.txt  %d ID_Start/ID_Continue lines" % props)
    print("   the table          %d runs of the class octet, %d upper, %d lower"
          % (runs, len(got_up), len(got_low)))

    bad = []
    wrong_cls = []
    for cp in range(MAX_CP):
        if ref_cls[cp] != got_cls[cp]:
            wrong_cls.append(cp)
            if len(wrong_cls) > 10000:
                break
    if wrong_cls:
        bad.append("   class octet DIFFERENT  %d code points, the first ten:" % len(wrong_cls))
        for cp in wrong_cls[:10]:
            a, b = ref_cls[cp], got_cls[cp]
            bad.append("     U+%04X  UCD cat %s id %d/%d  table cat %s id %d/%d"
                       % (cp, CATS[a & 31], (a >> 5) & 1, (a >> 6) & 1,
                          CATS[b & 31], (b >> 5) & 1, (b >> 6) & 1))
    n_up = compare_maps("upper", ref_up, got_up, bad)
    n_low = compare_maps("lower", ref_low, got_low, bad)

    if bad:
        print("\n".join(bad))
        return 1
    assigned = sum(1 for x in ref_cls if x & 31)
    starts = sum(1 for x in ref_cls if x & 32)
    conts = sum(1 for x in ref_cls if x & 64)
    print("   compared           %d code points (0 .. 0x10FFFF), nothing sampled"
          % MAX_CP)
    print("   IDENTICAL          category, ID_Start and ID_Continue for all "
          "%d code points" % MAX_CP)
    print("                      %d assigned, %d ID_Start, %d ID_Continue"
          % (assigned, starts, conts))
    print("   IDENTICAL          %d upper and %d lower case mappings, "
          "none missing, none invented" % (len(ref_up), len(ref_low)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
