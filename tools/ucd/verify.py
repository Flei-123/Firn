#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/ucd/verify.py -- holds the table the COMPILER generated against a
parser of its own, over ALL 1,114,112 code points.

Input: the standard output of `tools/ucd/ucd_real.fi`, one line per range:

    <lo> <hi> <Xy>

This script reads `UnicodeData.txt` again, entirely independently of the
`comptime` blocks -- in Python, with Python's own splitting of the fields --
and then compares, code point by code point, not by sampling:

  * every code point in the file has to have the same general category on
    both sides,
  * every code point NOT in the file has to be missing from the table as
    well (Cn, unassigned),
  * the ranges have to be ascending and must not overlap.

Usage:  python3 tools/ucd/verify.py <dump-file> [UnicodeData.txt]
Exit code 0 = identical, 1 = a difference (with the first ten printed).
"""
import os
import sys

MAX_CP = 0x110000


def reference(path):
    """cp -> category, straight out of the UCD, First/Last resolved."""
    table = {}
    pending = None
    lines = 0
    with open(path, "rb") as f:
        for raw in f:
            raw = raw.rstrip(b"\n")
            if not raw:
                continue
            fields = raw.split(b";")
            if len(fields) < 3:
                continue
            lines += 1
            cp = int(fields[0], 16)
            cat = fields[2].decode("ascii")
            name = fields[1]
            if name.endswith(b", First>"):
                pending = (cp, cat)
                continue
            if name.endswith(b", Last>"):
                if pending is None:
                    raise SystemExit("UCD: a `Last>` without a `First>` at %X" % cp)
                lo, lcat = pending
                if lcat != cat:
                    raise SystemExit("UCD: First/Last with different categories at %X" % cp)
                for x in range(lo, cp + 1):
                    table[x] = cat
                pending = None
                continue
            table[cp] = cat
    if pending is not None:
        raise SystemExit("UCD: a `First>` without a `Last>`")
    return table, lines


def generated(path):
    """cp -> category, out of the table the compiler produced."""
    table = {}
    ranges = 0
    last_hi = -1
    with open(path, encoding="ascii") as f:
        for nr, line in enumerate(f, 1):
            line = line.rstrip("\n")
            if not line:
                continue
            parts = line.split(" ")
            if len(parts) != 3:
                raise SystemExit("dump line %d has %d fields: %r" % (nr, len(parts), line))
            lo, hi, cat = int(parts[0]), int(parts[1]), parts[2]
            if len(cat) != 2:
                raise SystemExit("dump line %d: '%s' is no category" % (nr, cat))
            if lo > hi:
                raise SystemExit("dump line %d: %d > %d" % (nr, lo, hi))
            if lo <= last_hi:
                raise SystemExit("dump line %d: %d overlaps or is not ascending "
                                 "(previous end %d)" % (nr, lo, last_hi))
            if hi >= MAX_CP:
                raise SystemExit("dump line %d: %d is past the last code point" % (nr, hi))
            last_hi = hi
            ranges += 1
            for x in range(lo, hi + 1):
                table[x] = cat
    return table, ranges


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    dump = sys.argv[1]
    here = os.path.dirname(os.path.abspath(__file__))
    ucd = sys.argv[2] if len(sys.argv) > 2 else os.path.join(here, "UnicodeData.txt")

    ref, ucd_lines = reference(ucd)
    gen, ranges = generated(dump)

    wrong = []
    for cp in range(MAX_CP):
        a = ref.get(cp)
        b = gen.get(cp)
        if a != b:
            wrong.append((cp, a, b))
            if len(wrong) > 10000:
                break

    print("   UnicodeData.txt    %d lines, %d assigned code points"
          % (ucd_lines, len(ref)))
    print("   generated table    %d ranges, %d code points covered"
          % (ranges, len(gen)))
    print("   compared           %d code points (0 .. 0x10FFFF), nothing sampled"
          % MAX_CP)
    if wrong:
        print("   DIFFERENT          %d code points, the first ten:" % len(wrong))
        for cp, a, b in wrong[:10]:
            print("     U+%04X  UCD %s  table %s" % (cp, a or "Cn", b or "Cn"))
        return 1
    print("   IDENTICAL          all %d code points, %d assigned, "
          "%d unassigned (Cn)" % (MAX_CP, len(ref), MAX_CP - len(ref)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
