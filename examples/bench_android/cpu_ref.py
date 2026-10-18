#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# Reference implementation of the CPU benchmark (examples/bench_android/CPU_SPEC.md). Writes EXPECTED.txt lines.
import sys, time

M = 1_000_000_007

def lcg_seq(n, seed=12345):
    x = seed
    out = []
    for _ in range(n):
        x = (x * 1103515245 + 12345) & 0x7FFFFFFF
        out.append(x)
    return out

def primes(limit=5_000_000):
    s = bytearray([1]) * limit
    s[0] = s[1] = 0
    i = 2
    while i * i < limit:
        if s[i]:
            s[i * i::i] = bytes(len(range(i * i, limit, i)))
        i += 1
    count = 0
    total = 0
    for p in range(limit):
        if s[p]:
            count += 1
            total += p
    return (count * 1000003 + total) % M, count

def sort_cs(n=1_000_000):
    a = sorted(lcg_seq(n))
    t = 0
    for i, v in enumerate(a):
        t += v * ((i % 251) + 1)
    return t % M

def mandel(w=800, h=600, maxit=200):
    dx = 3.5 / 800.0
    dy = 2.0 / 600.0
    total = 0
    for py in range(h):
        ci = -1.0 + py * dy
        for px in range(w):
            cr = -2.5 + px * dx
            zr = 0.0
            zi = 0.0
            it = 0
            while it < maxit:
                zr2 = zr * zr
                zi2 = zi * zi
                if zr2 + zi2 > 4.0:
                    break
                zi = (2.0 * zr) * zi + ci
                zr = (zr2 - zi2) + cr
                it += 1
            total += it
    return total

if __name__ == "__main__":
    t = time.time()
    cs, cnt = primes()
    print("primes=%d (count %d)" % (cs, cnt), file=sys.stderr)
    s = sort_cs()
    m = mandel()
    print("primes=%d" % cs)
    print("sort=%d" % s)
    print("mandel=%d" % m)
    print("# primes count=%d, python %.1fs" % (cnt, time.time() - t), file=sys.stderr)
