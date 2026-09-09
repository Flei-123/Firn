#!/usr/bin/env python3
"""ROUND INLINE -- interleaved A/B wall clock, the method round SAMMLER used.

The machine carries the load of other rounds, so "run all of A, then all of B"
measures the load as much as it measures the binaries. This runs A,B,A,B,...
so the load hits both sides equally, reports the MEDIAN per side and, just as
important, HOW MANY of the pairs each side won: a 3 % median that rests on 8
of 15 pairs is noise, the same 3 % on 15 of 15 is a result.

Usage: ab.py <pairs> <stdin-file|-> <cmdA> -- <cmdB>
"""
import subprocess, sys, time, statistics

n = int(sys.argv[1])
inf = sys.argv[2]
rest = sys.argv[3:]
i = rest.index("--")
A, B = rest[:i], rest[i + 1:]
data = open(inf, "rb").read() if inf != "-" else b""

def once(cmd):
    t = time.perf_counter()
    subprocess.run(cmd, input=data, stdout=subprocess.DEVNULL,
                   stderr=subprocess.DEVNULL)
    return time.perf_counter() - t

ta, tb, wa = [], [], 0
for _ in range(n):
    da = once(A); db = once(B)
    ta.append(da); tb.append(db)
    if da < db:
        wa += 1
ma, mb = statistics.median(ta), statistics.median(tb)
print(f"A ({' '.join(A)}):  median {ma*1000:9.2f} ms   won {wa} of {n}")
print(f"B ({' '.join(B)}):  median {mb*1000:9.2f} ms   won {n-wa} of {n}")
print(f"B vs A: {(mb-ma)/ma*100:+.2f} %")
