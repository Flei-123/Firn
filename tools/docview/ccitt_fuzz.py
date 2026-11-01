#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
source = open(os.path.join(HERE, "ccitt_cross.py")).read().split("def main")[0]
exec(source)


def main():
    binary = sys.argv[1]
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 7
    iterations = int(sys.argv[3]) if len(sys.argv) > 3 else 120
    rng = random.Random(seed)
    bad = 0
    total = 0
    with tempfile.TemporaryDirectory() as work:
        for _ in range(iterations):
            w = rng.choice((8, 100, 400, 1728))
            h = rng.choice((5, 40))
            img = make_shapes(rng, w, h)
            for mode, (k, flags) in PARAMS.items():
                strips, rps, ref = encode(img, mode)
                raw = b"".join(strips)
                for _ in range(3):
                    b = bytearray(raw)
                    kind = rng.randrange(4)
                    if kind == 0:
                        b = b[:rng.randrange(1, len(b) + 1)]
                    elif kind == 1:
                        for _ in range(rng.randrange(1, 20)):
                            b[rng.randrange(len(b))] = rng.randrange(256)
                    elif kind == 2:
                        b = bytearray(rng.randrange(256) for _ in range(rng.randrange(1, 500)))
                    else:
                        i = rng.randrange(len(b))
                        del b[i:i + rng.randrange(1, 30)]
                    path = os.path.join(work, "f.bin")
                    with open(path, "wb") as f:
                        f.write(b)
                    for rows in (0, h):
                        total += 1
                        try:
                            r = subprocess.run([binary, path, str(k), str(w), str(rows), flags + "b"],
                                               capture_output=True, timeout=10)
                            if r.returncode not in (0, 4):
                                bad += 1
                                print("CRASH", mode, w, h, r.returncode)
                        except subprocess.TimeoutExpired:
                            bad += 1
                            print("HANG", mode, w, h)
    print("fuzz runs", total, "bad", bad)
    sys.exit(1 if bad else 0)


main()
