#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/av1/symtest.py <av1sym_main binary> <lib/av1/avif_cdf.fi> -- round trip of the AV1 symbol decoder (lib/av1/av1sym.fi).

A reference ENCODER (libaom od_ec_encode_q15 arithmetic, with Python big integers instead of carry propagation) writes random
symbols drawn from the real default mode CDFs, adapting them like the decoder must; the Firn decoder has to return the same
symbols from the bytes.  Also read_bool / read_literal.  Exit 0 when every run matches."""
import os, random, re, struct, subprocess, sys

def floor_log2(x):
    return x.bit_length() - 1

class Enc:
    def __init__(self):
        self.low = 0
        self.rng = 32768
        self.shift = 0

    def encode(self, cdf, n, s):          # cdf: list of n ascending values (spec form)
        icdf = [32768 - c for c in cdf[:n]]
        r = self.rng
        v = (((r >> 8) * (icdf[s] >> 6)) >> 1) + 4 * (n - s - 1)
        if s > 0:
            u = (((r >> 8) * (icdf[s - 1] >> 6)) >> 1) + 4 * (n - s)
        else:
            u = r
        self.low += r - u
        self.rng = u - v
        d = 15 - floor_log2(self.rng)
        self.low <<= d
        self.rng <<= d
        self.shift += d

    def finish(self):
        bits = 15 + self.shift
        nbytes = (bits + 7) // 8
        x = self.low << (nbytes * 8 - bits)
        return x.to_bytes(nbytes + 1, "big")[-nbytes:] if nbytes else b""

def adapt(cdf, n, s):
    cnt = cdf[n]
    rate = 3 + (cnt > 15) + (cnt > 31) + min(floor_log2(n), 2)
    tmp = 0
    for i in range(n - 1):
        if i == s:
            tmp = 32768
        if tmp < cdf[i]:
            cdf[i] -= (cdf[i] - tmp) >> rate
        else:
            cdf[i] += (tmp - cdf[i]) >> rate
    if cnt < 32:
        cdf[n] += 1

def load_tables(path):
    """cells of AV1_CDF_MODE from the generated lib/av1/avif_cdf.fi"""
    t = open(path).read()
    body = t[t.index("static AV1_CDF_MODE"):]
    body = body[body.index("[", body.index("=")) + 1:body.index("\n]")]
    return [int(x) for x in re.findall(r"\d+", body)]

def leaf_list(path):
    """(cell offset, natural symbol count) for every CDF leaf of the mode tables, in table order."""
    t = open(path).read()
    cells = load_tables(path)
    out = []
    for m in re.finditer(r"const M_(\w+): usize = (\d+)\nconst M_\1_N: usize = (\d+)\nconst M_\1_LEAVES: usize = (\d+)", t):
        off, n, cnt = int(m.group(2)), int(m.group(3)), int(m.group(4))
        for k in range(cnt):
            o = off + k * (n + 1)
            nat = cells[o:o + n + 1].index(32768) + 1
            out.append((o, nat))
    return out

def run_case(binary, cells, leaves, seed, nops, upd=True):
    rnd = random.Random(seed)
    work = list(cells)
    enc = Enc()
    ops, want = [], []
    for _ in range(nops):
        k = rnd.random()
        if k < 0.8:
            off, n = leaves[rnd.randrange(len(leaves))]
            cdf = work[off:off + n + 1]
            # draw from the current distribution most of the time, uniformly otherwise
            if rnd.random() < 0.7:
                x = rnd.randrange(32768)
                s = next(i for i in range(n) if x < cdf[i])
            else:
                s = rnd.randrange(n)
            enc.encode(cdf, n, s)
            adapt(cdf, n, s)
            work[off:off + n + 1] = cdf
            ops.append((0, n, off)); want.append(s)
        elif k < 0.9:
            b = rnd.randrange(2)
            enc.encode([16384, 32768], 2, b)
            ops.append((1, 0, 0)); want.append(b)
        else:
            n = rnd.randrange(1, 8)
            v = rnd.randrange(1 << n)
            for i in range(n - 1, -1, -1):
                enc.encode([16384, 32768], 2, (v >> i) & 1)
            ops.append((2, n, 0)); want.append(v)
    data = enc.finish()
    blob = struct.pack("<II", len(ops), len(data)) + data + b"".join(struct.pack("<BBH", a, b, c) for a, b, c in ops)
    r = subprocess.run([binary], input=blob, capture_output=True, timeout=60)
    got = list(r.stdout)
    if r.returncode != 0 or got != want:
        bad = next((i for i, (a, b) in enumerate(zip(got, want)) if a != b), min(len(got), len(want)))
        return "seed %d: rc=%d first mismatch at op %d (got %s want %s)" % (seed, r.returncode, bad, got[bad:bad + 1], want[bad:bad + 1])
    return None

def main():
    binary, src = sys.argv[1], sys.argv[2]
    cells = load_tables(src)
    leaves = leaf_list(src)
    bad = 0
    for seed, nops in [(1, 1), (2, 5), (3, 50), (4, 400), (5, 3000), (6, 3000), (7, 20000)]:
        e = run_case(binary, cells, leaves, seed, nops)
        print(("FAIL " + e) if e else "ok   seed %d ops %d" % (seed, nops))
        bad += bool(e)
    sys.exit(1 if bad else 0)

main()
