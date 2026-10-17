#!/usr/bin/env python3
"""std.textfmt against Python: '%.Nf' % x and round(x, N). Usage: cross.py gen N SEED OUT | check IN OUT"""
import random
import struct
import sys


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def gen(n, seed, path):
    r = random.Random(seed)
    cases = []
    ties = [0.5, 1.5, 2.5, 0.125, 0.375, 2.675, 1.005, 0.045, 1e-9, 5e-324, 1e15, 123456.789]
    for t in ties:
        for d in (0, 1, 2, 3, 6):
            cases.append((t, d))
            cases.append((-t, d))
    while len(cases) < n:
        k = r.randrange(6)
        if k == 0:
            x = r.uniform(-1000, 1000)
        elif k == 1:
            x = round(r.uniform(0, 5000), r.randrange(0, 5)) + r.choice([0, 0.5, 0.25, 0.125, 0.005])
        elif k == 2:
            x = r.random() * 10 ** r.randrange(-12, 12)
        elif k == 3:
            x = struct.unpack("<d", struct.pack("<Q", r.getrandbits(64)))[0]
            if x != x or x in (float("inf"), float("-inf")):
                continue
        elif k == 4:
            x = r.randrange(-10**6, 10**6) / 10 ** r.randrange(0, 6)
        else:
            x = r.randrange(0, 10**6) / 8
        cases.append((x, r.randrange(0, 8)))
    with open(path, "w") as f:
        for x, d in cases:
            b = bits(x)
            f.write(f"{b >> 32} {b & 0xFFFFFFFF} {d}\n")


def check(inp, outp):
    bad = skipped = total = 0
    for a, b in zip(open(inp), open(outp)):
        hi, lo, d = (int(v) for v in a.split())
        x = struct.unpack("<d", struct.pack("<Q", (hi << 32) | lo))[0]
        got_f, got_r = b.rstrip("\n").split("\t")
        want_f = "%.*f" % (d, x)
        want_r = str(bits(round(x, d)))
        total += 1
        if abs(x) * 10 ** d >= 2 ** 63:  # documented T2: not exact beyond this
            skipped += 1
            continue
        if got_f != want_f or got_r != want_r:
            bad += 1
            if bad <= 8:
                print(f"  MISMATCH x={x!r} d={d}: fixed {got_f!r} want {want_f!r}; round {got_r!r} want {want_r!r}")
    print(f"  textfmt cases: {total}, skipped (beyond T2): {skipped}, mismatches: {bad}")
    return 1 if bad else 0


if sys.argv[1] == "gen":
    gen(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4])
else:
    sys.exit(check(sys.argv[2], sys.argv[3]))
