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


# ---------------------------------------------------------------- kernels 4..9 (see CPU_SPEC.md)
def lcg_next(state):
    state[0] = (state[0] * 1103515245 + 12345) & 0x7FFFFFFF
    return state[0]

def hash_cs(n=4 * 1024 * 1024):
    import hashlib
    st = [12345]
    buf = bytearray(n)
    for i in range(n):
        buf[i] = (lcg_next(st) >> 16) & 255
    h = 2166136261
    for b in buf:
        h = ((h ^ b) * 16777619) & 0xFFFFFFFF
    d = hashlib.sha256(bytes(buf)).digest()
    w0 = int.from_bytes(d[:4], "big")
    return (h * 31 + w0) % M

def json_text(n=40000):
    st = [12345]
    parts = []
    for i in range(n):
        x = lcg_next(st)
        y = lcg_next(st)
        ok = "true" if i % 2 == 0 else "false"
        parts.append('{"id":%d,"v":%d,"name":"n%dx","f":[%d,%d,%d],"ok":%s}' % (i, x % 100000, i, y % 1000, y % 977, (y // 1000) % 1000, ok))
    return "[" + ",".join(parts) + "]"

def json_cs(n=40000):
    import json
    doc = json.loads(json_text(n))
    acc = {"num": 0, "str": 0, "obj": 0, "arr": 0, "true": 0}
    def walk(v):
        if v is True:
            acc["true"] += 1
        elif v is False or v is None:
            pass
        elif isinstance(v, int):
            acc["num"] += v
        elif isinstance(v, str):
            acc["str"] += len(v)
        elif isinstance(v, list):
            acc["arr"] += 1
            for e in v:
                walk(e)
        elif isinstance(v, dict):
            acc["obj"] += 1
            for k, e in v.items():
                acc["str"] += len(k)
                walk(e)
    walk(doc)
    return (acc["num"] + 7 * acc["str"] + 13 * acc["obj"] + 17 * acc["arr"] + 19 * acc["true"]) % M

def strbuild_cs(n=200000):
    s = "".join("item%d;" % i for i in range(n)).encode("ascii")
    t = len(s) * 31
    for j, c in enumerate(s):
        t += c * ((j % 7) + 1)
    return t % M

def matmul_cs(n=256):
    a4 = [[(i * 31 + j * 17) % 101 for j in range(n)] for i in range(n)]   # A = a4 * 0.25
    b2 = [[(i * 13 + j * 7) % 97 for j in range(n)] for i in range(n)]     # B = b2 * 0.5
    # total(C) * 8 = sum_ij sum_k a4[i][k] * b2[k][j]  (all products are exact binary fractions)
    t = 0
    for k in range(n):
        t += sum(a4[i][k] for i in range(n)) * sum(b2[k])
    return t % M

def hashmap_cs(n=300000):
    st = [12345]
    keys = [lcg_next(st) for _ in range(n)]
    m = {}
    for i, k in enumerate(keys):
        m[k] = i
    hits = 0
    total = 0
    for i in range(n):
        q = keys[(i * 7919) % n]
        if q in m:
            hits += 1
            total += m[q]
        q2 = keys[i] ^ 1
        if q2 in m:
            hits += 1
            total += m[q2]
    return (hits * 1000003 + total + len(m)) % M

def objsort_cs(n=200000):
    st = [12345]
    items = []
    for i in range(n):
        key = lcg_next(st) % 1000
        w = lcg_next(st)
        items.append((key, w, i))
    items.sort()
    t = 0
    for pos, it in enumerate(items):
        t += it[2] * ((pos % 251) + 1)
    return t % M

if __name__ == "__main__":
    t = time.time()
    cs, cnt = primes()
    print("primes=%d (count %d)" % (cs, cnt), file=sys.stderr)
    s = sort_cs()
    m = mandel()
    print("primes=%d" % cs)
    print("sort=%d" % s)
    print("mandel=%d" % m)
    print("hash=%d" % hash_cs())
    print("json=%d" % json_cs())
    print("strbuild=%d" % strbuild_cs())
    print("matmul=%d" % matmul_cs())
    print("hashmap=%d" % hashmap_cs())
    print("objsort=%d" % objsort_cs())
    print("# primes count=%d, python %.1fs" % (cnt, time.time() - t), file=sys.stderr)
