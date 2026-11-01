import os
import random
import struct
import subprocess
import sys
import tempfile


def flog2(x):
    return x.bit_length() - 1


class Enc:
    def __init__(self):
        self.low = 0
        self.rng = 32768
        self.shifts = 0

    def bit(self, s):
        r = self.rng
        cdf = [16384, 32768]
        fh = 32768 - cdf[s]
        v = ((r >> 8) * (fh >> 6) >> 1) + 4 * (1 - s)
        if s > 0:
            fl = 32768 - cdf[s - 1]
            u = ((r >> 8) * (fl >> 6) >> 1) + 4 * (1 - (s - 1))
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


def golomb_bits(v):
    x = v + 1
    n = x.bit_length()
    bits = [0] * (n - 1) + [1]
    bits += [(x >> i) & 1 for i in range(n - 2, -1, -1)]
    return bits


def run(exe, seed):
    rnd = random.Random(seed)
    vals = [rnd.choice([0, 1, 2, 3, 7, 100, 1000, 40000, rnd.randint(0, 60000)]) for _ in range(rnd.randint(1, 200))]
    enc = Enc()
    for v in vals:
        for b in golomb_bits(v):
            enc.bit(b)
    blob = struct.pack("<%dH" % (len(vals) + 1), len(vals), *vals) + enc.finish()
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
    rc = run(exe, seed)
    if rc != 0:
        bad += 1
        print("seed", seed, "rc", rc)
print("av1 golomb roundtrips bad=%d" % bad)
sys.exit(1 if bad else 0)
