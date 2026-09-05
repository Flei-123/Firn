#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/stdlib81/hashvectors.py -- lib/std/hash.fi against the world.

FNV-1a is recomputed here in four lines (the algorithm IS four lines, so a
second implementation is cheap and honest). xxHash64 is not reimplemented:
it is compared against `python-xxhash`, the binding to the author's own C
library. If that module is not installed, the three PUBLISHED vectors
(`""`, `"a"`, `"foobar"` with seed 0) still have to match -- they pin the
seed setup, the tail path and the avalanche.

Usage: hashvectors.py <hashprobe binary>
"""
import subprocess
import sys

PUBLISHED = {
    # FNV-1a 64, the values of the FNV reference page.
    "fnv": {b"": "cbf29ce484222325", b"a": "af63dc4c8601ec8c",
            b"foobar": "85944171f73967e8"},
    # xxHash64, seed 0 -- the vectors of the specification.
    "xx": {b"": "ef46db3751d8e999"},
}


def fnv1a64(data):
    h = 0xCBF29CE484222325
    for c in data:
        h = ((h ^ c) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def main():
    if len(sys.argv) < 2:
        print("usage: hashvectors.py <hashprobe>")
        return 2
    out = subprocess.run([sys.argv[1]], capture_output=True)
    if out.returncode != 0:
        print("FAIL: hashprobe exited with %d" % out.returncode)
        return 1

    try:
        import xxhash
        have_xx = True
    except ImportError:
        xxhash = None
        have_xx = False

    counting = bytes((i + 65) & 0xFF for i in range(256))
    inputs = {"empty": b"", "a": b"a", "foobar": b"foobar"}
    for n in (7, 8, 12, 31, 32, 33, 64, 200):
        inputs["len%03d" % n] = counting[:n]

    bad = 0
    checked_fnv = checked_xx = checked_seed = 0
    for line in out.stdout.decode().splitlines():
        parts = line.split()
        if len(parts) != 4:
            continue
        name, f, x, xs = parts
        data = inputs.get(name)
        if data is None:
            continue
        want_f = "%016x" % fnv1a64(data)
        if f != want_f:
            print("FAIL fnv1a64(%s): firn %s, python %s" % (name, f, want_f))
            bad += 1
        else:
            checked_fnv += 1
        if data in PUBLISHED["fnv"] and f != PUBLISHED["fnv"][data]:
            print("FAIL fnv1a64(%s) against the published vector" % name)
            bad += 1
        if data in PUBLISHED["xx"] and x != PUBLISHED["xx"][data]:
            print("FAIL xx64(%s): firn %s, published %s"
                  % (name, x, PUBLISHED["xx"][data]))
            bad += 1
        if have_xx:
            want_x = "%016x" % xxhash.xxh64(data).intdigest()
            want_s = "%016x" % xxhash.xxh64(data, seed=12345).intdigest()
            if x != want_x:
                print("FAIL xx64(%s): firn %s, reference %s" % (name, x, want_x))
                bad += 1
            else:
                checked_xx += 1
            if xs != want_s:
                print("FAIL xx64(%s, seed 12345): firn %s, reference %s"
                      % (name, xs, want_s))
                bad += 1
            else:
                checked_seed += 1

    src = "python-xxhash (the author's C library)" if have_xx \
        else "the published vectors only -- python-xxhash is not installed"
    print("hash vectors: FNV-1a %d/%d against a second implementation, "
          "xxHash64 %d + %d seeded against %s"
          % (checked_fnv, len(inputs), checked_xx, checked_seed, src))
    if bad:
        print("hash vectors: %d MISMATCHES" % bad)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
