import os
import random
import struct
import subprocess
import sys
import tempfile


def flog2(x):
    return x.bit_length() - 1


def adapt(cdf, n, sym):
    rate = 3 + (cdf[n] > 15) + (cdf[n] > 31) + min(flog2(n), 2)
    tmp = 0
    for i in range(n - 1):
        if i == sym:
            tmp = 32768
        if tmp < cdf[i]:
            cdf[i] -= (cdf[i] - tmp) >> rate
        else:
            cdf[i] += (tmp - cdf[i]) >> rate
    if cdf[n] < 32:
        cdf[n] += 1


class Enc:
    def __init__(self):
        self.low = 0
        self.rng = 32768
        self.shifts = 0

    def encode(self, cdf, n, s):
        r = self.rng
        fh = 32768 - cdf[s]
        v = ((r >> 8) * (fh >> 6) >> 1) + 4 * (n - 1 - s)
        if s > 0:
            fl = 32768 - cdf[s - 1]
            u = ((r >> 8) * (fl >> 6) >> 1) + 4 * (n - 1 - (s - 1))
            self.low += r - u
            self.rng = u - v
        else:
            self.rng = r - v
        d = 15 - flog2(self.rng)
        self.rng <<= d
        self.low <<= d
        self.shifts += d

    def finish(self):
        top = self.low + self.rng - 1
        x = self.low ^ top
        k = x.bit_length()
        v = top if k == 0 else (top >> (k - 1)) << (k - 1)
        total = 15 + self.shifts
        pad = (-total) % 8
        v <<= pad
        total += pad
        return v.to_bytes(total // 8, "big")


def run(exe, seed, nsyms):
    rnd = random.Random(seed)
    nctx = rnd.randint(1, 8)
    ctxs = []
    for _ in range(nctx):
        n = rnd.randint(2, 16)
        cuts = sorted(rnd.sample(range(1, 32768), n - 1))
        ctxs.append((n, cuts + [32768, 0]))
    init = [(n, list(c)) for n, c in ctxs]
    enc = Enc()
    syms = []
    for _ in range(nsyms):
        if rnd.random() < 0.15:
            b = rnd.randint(0, 1)
            enc.encode([16384, 32768, 0], 2, b)
            syms.append((65535, b))
        else:
            ci = rnd.randrange(nctx)
            n, c = ctxs[ci]
            s = rnd.choices(range(n), weights=[1 + (i == 0) * 6 for i in range(n)])[0]
            enc.encode(c, n, s)
            adapt(c, n, s)
            syms.append((ci, s))
    stream = enc.finish()
    words = [len(init)]
    for n, c in init:
        words.append(n)
        words.extend(c)
    words.append(len(syms) & 65535)
    words.append(len(syms) >> 16)
    for ci, s in syms:
        words.extend([ci, s])
    blob = struct.pack("<%dH" % len(words), *words) + stream
    d = tempfile.mkdtemp()
    p = os.path.join(d, "t.bin")
    open(p, "wb").write(blob)
    rc = subprocess.run([exe, p]).returncode
    os.remove(p)
    os.rmdir(d)
    return rc


exe = sys.argv[1]
bad = 0
for seed in range(int(sys.argv[2])):
    rc = run(exe, seed, random.Random(seed + 1000).choice([1, 5, 50, 500, 3000]))
    if rc != 0:
        bad += 1
        print("seed", seed, "rc", rc)
print("av1 symbol decoder roundtrips bad=%d" % bad)
sys.exit(1 if bad else 0)
