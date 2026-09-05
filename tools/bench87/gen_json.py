#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/bench87/gen_json.py -- the two documents of the JSON measurement.

Same shape, same number of members, same key names: the ONLY difference is
that one carries integers where the other carries floating point numbers.
That is what makes the two throughputs comparable at all -- everything the
reader does apart from the number is identical.

    gen_json.py <outdir> [objects]

writes <outdir>/int.json and <outdir>/float.json.
"""
import json
import os
import random
import sys


def main() -> int:
    out = sys.argv[1]
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 20000
    os.makedirs(out, exist_ok=True)
    rnd = random.Random(20870823)
    ints, floats = [], []
    for i in range(n):
        a = rnd.randint(-2_000_000, 2_000_000)
        b = rnd.randint(0, 1_000_000)
        c = rnd.randint(0, 999)
        base = {"id": i, "name": "item-%06d" % i, "tag": "abcdefgh"[i % 8],
                "ok": bool(i & 1)}
        ints.append(dict(base, x=a, y=b, z=c))
        # the same numbers, only with a decimal point and an exponent -- the
        # sort of thing a real document carries
        floats.append(dict(base,
                           x=round(a / 1024.0, 6),
                           y=round(b / 7.0, 8),
                           z=float("%de-3" % c)))
    for name, doc in (("int", ints), ("float", floats)):
        p = os.path.join(out, name + ".json")
        with open(p, "w") as fh:
            json.dump(doc, fh, separators=(",", ":"))
        print("%s  %d bytes" % (p, os.path.getsize(p)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
