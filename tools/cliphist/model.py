#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/cliphist/model.py -- the oracle for lib/std/cliphist.fi: an independent Python model of the history rules
# and of the file format (incl. XXH64, written out here), driven by random operation lists.
#
#   model.py gen <seed> <ops> <ops file> <expected file>     random operations + what the model says
#   model.py cmp <expected file> <result file>               compare with what check_main.fi printed
#
# Operation lines (hex = UTF-8 octets as hex, "-" = empty):
#   A <text hex> <html hex> <sensitive 0|1> <time>   add          P <k>   toggle pin of display entry k
#   D <k>  delete display entry k      C <0|1>  clear (1 = keep pinned)      X <time>  expire
#   L <n>  limit      S <ms>  sensitive ttl      M <n>  max entry size
# Result file: after the last operation "E <pinned> <sens> <time> <text hex> <html hex>" per entry in DISPLAY order,
# then "F <hex of the file image>".
import random, struct, sys

M64 = (1 << 64) - 1
P1, P2, P3, P4, P5 = 11400714785074694791, 14029467366897019727, 1609587929392839161, 9650029242287828579, 2870177450012600261

def rotl(x, r): return ((x << r) | (x >> (64 - r))) & M64
def rnd(acc, inp): return (rotl((acc + inp * P2) & M64, 31) * P1) & M64
def merge(acc, val): return (((acc ^ rnd(0, val)) * P1) + P4) & M64

def xxh64(data, seed=0):
    n = len(data); i = 0
    if n >= 32:
        v1 = (seed + P1 + P2) & M64; v2 = (seed + P2) & M64; v3 = seed & M64; v4 = (seed - P1) & M64
        while i + 32 <= n:
            a, b, c, d = struct.unpack_from("<4Q", data, i)
            v1, v2, v3, v4 = rnd(v1, a), rnd(v2, b), rnd(v3, c), rnd(v4, d); i += 32
        h = (rotl(v1, 1) + rotl(v2, 7) + rotl(v3, 12) + rotl(v4, 18)) & M64
        for v in (v1, v2, v3, v4): h = merge(h, v)
    else:
        h = (seed + P5) & M64
    h = (h + n) & M64
    while i + 8 <= n:
        k = struct.unpack_from("<Q", data, i)[0]; h = (rotl(h ^ rnd(0, k), 27) * P1 + P4) & M64; i += 8
    if i + 4 <= n:
        k = struct.unpack_from("<I", data, i)[0]; h = (rotl(h ^ ((k * P1) & M64), 23) * P2 + P3) & M64; i += 4
    while i < n:
        h = (rotl(h ^ ((data[i] * P5) & M64), 11) * P1) & M64; i += 1
    h ^= h >> 33; h = (h * P2) & M64; h ^= h >> 29; h = (h * P3) & M64; h ^= h >> 32
    return h

class Hist:
    def __init__(self):
        self.items = []  # newest first; dict(text, html, png, pinned, sens, time, exp)
        self.limit, self.max, self.ttl = 25, 4194304, 30000
    def enforce(self):
        while True:
            un = [i for i, e in enumerate(self.items) if not e["pinned"]]
            if len(un) <= self.limit: return
            del self.items[un[-1]]
    def add(self, text, html, sens, t):
        n = len(text) + len(html)
        if n == 0 or n > self.max: return
        if sens and self.ttl <= 0: return
        for i, e in enumerate(self.items):
            if e["text"] == text and e["html"] == html:
                e["time"] = t
                if sens: e["sens"] = True
                if e["sens"]: e["exp"] = t + self.ttl
                self.items.insert(0, self.items.pop(i)); return
        self.items.insert(0, dict(text=text, html=html, pinned=False, sens=sens, time=t, exp=(t + self.ttl) if sens else 0))
        self.enforce()
    def display(self):
        return [e for e in self.items if e["pinned"]] + [e for e in self.items if not e["pinned"]]
    def pin(self, k):
        d = self.display()
        if k >= len(d): return
        d[k]["pinned"] = not d[k]["pinned"]
        self.enforce()
    def delete(self, k):
        d = self.display()
        if k >= len(d): return
        self.items = [e for e in self.items if e is not d[k]]
    def clear(self, keep):
        self.items = [e for e in self.items if keep and e["pinned"]]
    def expire(self, t):
        self.items = [e for e in self.items if not (e["sens"] and t >= e["exp"])]
    def image(self):
        body = [e for e in self.items if not e["sens"]]
        out = b"FCH1" + struct.pack("<I", len(body))
        for e in body:
            out += struct.pack("<QIIII", e["time"] & M64, 1 if e["pinned"] else 0, len(e["text"]), len(e["html"]), 0)
            out += e["text"] + e["html"]
        return out + struct.pack("<Q", xxh64(out))

def hx(b): return b.hex() if b else "-"
def unhx(s): return b"" if s == "-" else bytes.fromhex(s)

WORDS = ["a", "b", "hello", "wörld", "pass", "https://example.org/x", "line1\nline2", "😀 emoji", "x" * 40, "tab\tsep", ""]

def gen(seed, nops, ops_path, exp_path):
    r = random.Random(seed); h = Hist(); ops = []; t = 1000
    for _ in range(nops):
        t += r.randint(0, 4000)
        k = r.random()
        if k < 0.55:
            text = r.choice(WORDS).encode(); html = (r.choice(["", "", "<b>%s</b>" % r.choice(WORDS[:5])])).encode()
            if r.random() < 0.05: text = bytes(r.randint(0, 255) for _ in range(r.randint(1, 12)))
            sens = 1 if r.random() < 0.15 else 0
            ops.append("A %s %s %d %d" % (hx(text), hx(html), sens, t)); h.add(text, html, bool(sens), t)
        elif k < 0.68:
            n = r.randint(0, 6); ops.append("P %d" % n); h.pin(n)
        elif k < 0.78:
            n = r.randint(0, 6); ops.append("D %d" % n); h.delete(n)
        elif k < 0.81:
            c = r.randint(0, 1); ops.append("C %d" % c); h.clear(bool(c))
        elif k < 0.90:
            ops.append("X %d" % t); h.expire(t)
        elif k < 0.94:
            n = r.randint(1, 8); ops.append("L %d" % n); h.limit = n; h.enforce()
        elif k < 0.97:
            n = r.choice([0, 1000, 30000, 60000]); ops.append("S %d" % n); h.ttl = n
        else:
            n = r.choice([8, 30, 4194304]); ops.append("M %d" % n); h.max = n
    open(ops_path, "w").write("\n".join(ops) + "\n")
    lines = ["E %d %d %d %s %s" % (e["pinned"], e["sens"], e["time"], hx(e["text"]), hx(e["html"])) for e in h.display()]
    lines.append("F " + h.image().hex())
    open(exp_path, "w").write("\n".join(lines) + "\n")

def cmp(exp_path, res_path):
    exp = open(exp_path).read().split("\n"); res = open(res_path).read().split("\n")
    diffs = 0 if exp == res else 1
    if diffs:
        for a, b in zip(exp, res):
            if a != b:
                print("  expected:", a[:160]); print("  got     :", b[:160]); break
        if len(exp) != len(res): print("  line counts differ: %d vs %d" % (len(exp), len(res)))
    print("%d differences" % diffs)
    return diffs

if __name__ == "__main__":
    if sys.argv[1] == "gen": gen(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4], sys.argv[5])
    else: sys.exit(1 if cmp(sys.argv[2], sys.argv[3]) else 0)
