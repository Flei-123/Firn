#!/usr/bin/env python3
"""Independent reference for tools/mc/noise/dump.fi (everything except the java.util.Random lines).
Pure python ints / float64, same operation order as the Firn code, so results agree bit for bit."""
import math, struct

M64 = (1 << 64) - 1

def hexf(v):
    if v == 0.0:
        v = 0.0
    return "%016x" % struct.unpack(">Q", struct.pack(">d", v))[0]

def splitmix(seed):
    s = seed & M64
    while True:
        s = (s + 0x9E3779B97F4A7C15) & M64
        z = s
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M64
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M64
        yield z ^ (z >> 31)

def rotl(x, k):
    return ((x << k) | (x >> (64 - k))) & M64

def xoshiro(seed):
    g = splitmix(seed)
    s = [next(g) for _ in range(4)]
    while True:
        result = (rotl((s[1] * 5) & M64, 7) * 9) & M64
        t = (s[1] << 17) & M64
        s[2] ^= s[0]; s[3] ^= s[1]; s[1] ^= s[2]; s[0] ^= s[3]; s[2] ^= t; s[3] = rotl(s[3], 45)
        yield result

class JRandom:  # only for the seeded permutation (the real thing is checked against java itself)
    def __init__(self, seed):
        self.seed = (seed ^ 0x5DEECE66D) & ((1 << 48) - 1)
    def next(self, bits):
        self.seed = (self.seed * 0x5DEECE66D + 0xB) & ((1 << 48) - 1)
        v = self.seed >> (48 - bits)
        return v - (1 << 32) if v >= (1 << 31) and bits == 32 else v
    def next_int_bound(self, bound):
        r = self.next(31)
        m = bound - 1
        if bound & m == 0:
            return (bound * r) >> 31
        u = r
        r = u % bound
        while u - r + m >= (1 << 31):
            u = self.next(31)
            r = u % bound
        return r

REF = [151,160,137,91,90,15,131,13,201,95,96,53,194,233,7,225,140,36,103,30,69,142,8,99,37,240,21,10,23,190,6,148,247,120,234,75,0,26,197,62,94,252,219,203,117,35,11,32,57,177,33,88,237,149,56,87,174,20,125,136,171,168,68,175,74,165,71,134,139,48,27,166,77,146,158,231,83,111,229,122,60,211,133,230,220,105,92,41,55,46,245,40,244,102,143,54,65,25,63,161,1,216,80,73,209,76,132,187,208,89,18,169,200,196,135,130,116,188,159,86,164,100,109,198,173,186,3,64,52,217,226,250,124,123,5,202,38,147,118,126,255,82,85,212,207,206,59,227,47,16,58,17,182,189,28,42,223,183,170,213,119,248,152,2,44,154,163,70,221,153,101,155,167,43,172,9,129,22,39,253,19,98,108,110,79,113,224,232,178,185,112,104,218,246,97,228,251,34,242,193,238,210,144,12,191,179,162,241,81,51,145,235,249,14,239,107,49,192,214,31,181,199,106,157,184,84,204,176,115,121,50,45,127,4,150,254,138,236,205,93,222,114,67,29,24,72,243,141,128,195,78,66,215,61,156,180]
assert sorted(REF) == list(range(256))

def perm_ref():
    return REF + REF

def perm_seeded(seed):
    t = list(range(256))
    g = JRandom(seed)
    for i in range(256):
        j = i + g.next_int_bound(256 - i)
        t[i], t[j] = t[j], t[i]
    return t + t

def fade(t): return t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
def lerp(t, a, b): return a + t * (b - a)

def grad3(h, x, y, z):
    h &= 15
    u = x if h < 8 else y
    v = y if h < 4 else (x if h == 12 or h == 14 else z)
    return (-u if h & 1 else u) + (-v if h & 2 else v)

def perlin3(p, x0, y0, z0):
    fx, fy, fz = math.floor(x0), math.floor(y0), math.floor(z0)
    X, Y, Z = int(fx) & 255, int(fy) & 255, int(fz) & 255
    x, y, z = x0 - fx, y0 - fy, z0 - fz
    u, v, w = fade(x), fade(y), fade(z)
    A = p[X] + Y; AA = p[A] + Z; AB = p[A + 1] + Z
    B = p[X + 1] + Y; BA = p[B] + Z; BB = p[B + 1] + Z
    return lerp(w, lerp(v, lerp(u, grad3(p[AA], x, y, z), grad3(p[BA], x - 1.0, y, z)),
                           lerp(u, grad3(p[AB], x, y - 1.0, z), grad3(p[BB], x - 1.0, y - 1.0, z))),
                   lerp(v, lerp(u, grad3(p[AA + 1], x, y, z - 1.0), grad3(p[BA + 1], x - 1.0, y, z - 1.0)),
                           lerp(u, grad3(p[AB + 1], x, y - 1.0, z - 1.0), grad3(p[BB + 1], x - 1.0, y - 1.0, z - 1.0))))

G3 = [(1,1,0),(-1,1,0),(1,-1,0),(-1,-1,0),(1,0,1),(-1,0,1),(1,0,-1),(-1,0,-1),(0,1,1),(0,-1,1),(0,1,-1),(0,-1,-1)]
def dot3(g, x, y, z): return g[0] * x + g[1] * y + g[2] * z

def simplex2(p, xin, yin):
    F2 = 0.5 * (math.sqrt(3.0) - 1.0); G2 = (3.0 - math.sqrt(3.0)) / 6.0
    s = (xin + yin) * F2
    i = math.floor(xin + s); j = math.floor(yin + s)
    t = (i + j) * G2
    x0 = xin - (i - t); y0 = yin - (j - t)
    if x0 > y0: i1, j1 = 1, 0
    else: i1, j1 = 0, 1
    x1 = x0 - i1 + G2; y1 = y0 - j1 + G2
    x2 = x0 - 1.0 + 2.0 * G2; y2 = y0 - 1.0 + 2.0 * G2
    ii = i & 255; jj = j & 255
    gi0 = p[ii + p[jj]] % 12; gi1 = p[ii + i1 + p[jj + j1]] % 12; gi2 = p[ii + 1 + p[jj + 1]] % 12
    n0 = n1 = n2 = 0.0
    t0 = 0.5 - x0 * x0 - y0 * y0
    if t0 >= 0: t0 *= t0; n0 = t0 * t0 * dot3(G3[gi0], x0, y0, 0.0)
    t1 = 0.5 - x1 * x1 - y1 * y1
    if t1 >= 0: t1 *= t1; n1 = t1 * t1 * dot3(G3[gi1], x1, y1, 0.0)
    t2 = 0.5 - x2 * x2 - y2 * y2
    if t2 >= 0: t2 *= t2; n2 = t2 * t2 * dot3(G3[gi2], x2, y2, 0.0)
    return 70.0 * (n0 + n1 + n2)

def simplex3(p, xin, yin, zin):
    F3 = 1.0 / 3.0; G3c = 1.0 / 6.0
    s = (xin + yin + zin) * F3
    i = math.floor(xin + s); j = math.floor(yin + s); k = math.floor(zin + s)
    t = (i + j + k) * G3c
    x0 = xin - (i - t); y0 = yin - (j - t); z0 = zin - (k - t)
    if x0 >= y0:
        if y0 >= z0: i1, j1, k1, i2, j2, k2 = 1, 0, 0, 1, 1, 0
        elif x0 >= z0: i1, j1, k1, i2, j2, k2 = 1, 0, 0, 1, 0, 1
        else: i1, j1, k1, i2, j2, k2 = 0, 0, 1, 1, 0, 1
    else:
        if y0 < z0: i1, j1, k1, i2, j2, k2 = 0, 0, 1, 0, 1, 1
        elif x0 < z0: i1, j1, k1, i2, j2, k2 = 0, 1, 0, 0, 1, 1
        else: i1, j1, k1, i2, j2, k2 = 0, 1, 0, 1, 1, 0
    x1 = x0 - i1 + G3c; y1 = y0 - j1 + G3c; z1 = z0 - k1 + G3c
    x2 = x0 - i2 + 2.0 * G3c; y2 = y0 - j2 + 2.0 * G3c; z2 = z0 - k2 + 2.0 * G3c
    x3 = x0 - 1.0 + 3.0 * G3c; y3 = y0 - 1.0 + 3.0 * G3c; z3 = z0 - 1.0 + 3.0 * G3c
    ii = i & 255; jj = j & 255; kk = k & 255
    gi0 = p[ii + p[jj + p[kk]]] % 12
    gi1 = p[ii + i1 + p[jj + j1 + p[kk + k1]]] % 12
    gi2 = p[ii + i2 + p[jj + j2 + p[kk + k2]]] % 12
    gi3 = p[ii + 1 + p[jj + 1 + p[kk + 1]]] % 12
    n = [0.0] * 4
    for idx, (gi, x, y, z) in enumerate(((gi0, x0, y0, z0), (gi1, x1, y1, z1), (gi2, x2, y2, z2), (gi3, x3, y3, z3))):
        t = 0.6 - x * x - y * y - z * z
        if t >= 0:
            t *= t
            n[idx] = t * t * dot3(G3[gi], x, y, z)
    return 32.0 * (n[0] + n[1] + n[2] + n[3])

def octaves(fn, args, oc, lac, pers):
    s = 0.0; amp = 1.0; freq = 1.0; norm = 0.0
    for _ in range(oc):
        s = s + amp * fn(*[a * freq for a in args])
        norm = norm + amp
        amp = amp * pers; freq = freq * lac
    return 0.0 if norm == 0.0 else s / norm

def main():
    out = []
    g = splitmix(0)
    for _ in range(5): out.append("sm0 %016x" % next(g))
    g = splitmix(1234567)
    for _ in range(5): out.append("sm1 %016x" % next(g))
    g = xoshiro(42)
    for _ in range(8): out.append("xo %016x" % next(g))
    pr = perm_ref()
    out.append("pr 3.14 42 7 " + hexf(perlin3(pr, 3.14, 42.0, 7.0)))
    for k in range(12):
        x = k * 0.37 - 2.0; y = k * 0.53 + 0.11; z = 0.0 - k * 0.29
        out.append("p3 %d %s %s %s %s" % (k, hexf(perlin3(pr, x, y, z)), hexf(perlin3(pr, x, y, 0.0)), hexf(simplex2(pr, x, y)), hexf(simplex3(pr, x, y, z))))
    ps = perm_seeded(12345)
    for k in range(6):
        x = k * 1.7 + 0.3
        out.append("ps %d %s %s %s" % (k, hexf(perlin3(ps, x, x * 0.5, 0.25)),
                   hexf(octaves(lambda a, b, c: perlin3(ps, a, b, c), (x, x * 0.5, 0.25), 4, 2.0, 0.5)),
                   hexf(octaves(lambda a, b: simplex2(ps, a, b), (x, x * 0.7), 5, 2.0, 0.5))))
    print("\n".join(out))

main()
