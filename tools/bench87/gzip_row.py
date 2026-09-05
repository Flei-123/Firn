#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/bench87/gzip_row.py -- one line of the level table of run.sh.

Gets Firn's measured microseconds and packed size handed in and measures
`gzip` for the same level itself, on the same octets. `zlib.compress`
delivers the SIZE (six octets of zlib frame taken off, so that raw stands
against raw), `gzip -N` on a pipe delivers the TIME -- the two are the same
implementation, and each is asked what it is good at.
"""
import subprocess
import sys
import time
import zlib


def main() -> int:
    L, size, reps, us, packed, path, runs = sys.argv[1:8]
    L, size, reps = int(L), int(size), int(reps)
    us, packed, runs = int(us), int(packed), int(runs)
    data = open(path, "rb").read()
    best = 1e9
    for _ in range(runs):
        t = time.monotonic()
        subprocess.run(["gzip", "-%d" % L, "-c"], input=data,
                       stdout=subprocess.DEVNULL)
        best = min(best, time.monotonic() - t)
    theirs = len(zlib.compress(data, L)) - 6
    print("  %-5s %13.2f %13d %12.2f %13d"
          % ("-%d" % L, (size * reps / 1048576.0) / (us / 1e6), packed,
             (size / 1048576.0) / best, theirs))
    return 0


if __name__ == "__main__":
    sys.exit(main())
