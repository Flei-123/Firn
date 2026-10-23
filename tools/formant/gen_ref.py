#!/usr/bin/env python3
"""tools/formant/gen_ref.py -- reference values for tests/4962_formant_ref_python.fi.

    python3 -I tools/formant/gen_ref.py                 print the Firn constants
    python3 -I tools/formant/gen_ref.py --patch FILE    rewrite the block between "// REF-BEGIN" and "// REF-END" in FILE
    python3 -I tools/formant/gen_ref.py --check FILE    exit 1 if that block is out of date
    python3 -I tools/formant/gen_ref.py --report        print the deviation Firn (table sine) vs scipy (see below)

TWO KINDS OF REFERENCE
 1. EXACT: a line-by-line port of lib/audio/{synth,synth_osc,synth_fx,formant,formant_cons}.fi in plain Python floats.
    Firn computes with + - * / on f64 only (no libm, no FMA), Python floats are the same IEEE doubles, so the S16
    output must be BIT IDENTICAL: the test compares the CRC-32 of the S16 samples (zlib.crc32 == audio.pcm.crc32).
 2. INDEPENDENT: scipy.signal.lfilter with the textbook RBJ band-pass coefficients computed with math.sin / math.cos
    (not the table sine of Firn). The difference of the two is the error of the 4096-point sine table in the
    coefficients; the test allows a tolerance and --report prints the measured maximum.
numpy / scipy are used only for kind 2; the exact port is pure Python."""
import math
import re
import sys
import zlib

M64 = (1 << 64) - 1


# ------------------------------------------------------------------ synth.fi (integer CORDIC)
def atan_turn(i):
    return [536870912, 316933406, 167458907, 85004756, 42667331, 21354465, 10679838, 5340245, 2670163, 1335087,
            667544, 333772, 166886, 83443, 41722, 20861, 10430, 5215, 2608, 1304, 652, 326, 163, 81][i]


def sine_q30(phase):
    ph = phase & 0xFFFFFFFF
    quad = ph >> 30
    z = ph & 1073741823
    x = 652032874
    y = 0
    for i in range(24):
        dx = y >> i
        dy = x >> i
        if z >= 0:
            x = x - dx
            y = y + dy
            z = z - atan_turn(i)
        else:
            x = x + dx
            y = y - dy
            z = z + atan_turn(i)
    if quad == 0:
        return y
    if quad == 1:
        return x
    if quad == 2:
        return -y
    return -x


# ------------------------------------------------------------------ synth_osc.fi
SIN_TAB = [sine_q30((i * 1048576) & 0xFFFFFFFF) / 1073741824.0 for i in range(4097)]
LN2 = 0.6931471805599453


def ffloor(x):
    i = int(x)
    f = float(i)
    if f > x:
        return f - 1.0
    return f


def fract(x):
    return x - ffloor(x)


def pow2(x0):
    x = x0
    if x > 80.0:
        x = 80.0
    if x < -80.0:
        x = -80.0
    n = ffloor(x)
    e = (x - n) * LN2
    term = 1.0
    s = 1.0
    for k in range(1, 15):
        term = term * e / float(k)
        s = s + term
    ni = int(n)
    while ni > 0:
        s = s * 2.0
        ni -= 1
    while ni < 0:
        s = s * 0.5
        ni += 1
    return s


def sin_turn(t):
    f = fract(t) * 4096.0
    i = int(f)
    w = f - float(i)
    a = SIN_TAB[i]
    b = SIN_TAB[i + 1]
    return a + (b - a) * w


def cos_turn(t):
    return sin_turn(t + 0.25)


class Rng:
    def __init__(self, seed):
        z = (seed + 11400714819323198485) & M64
        z = ((z ^ (z >> 30)) * 13787848793156543929) & M64
        z = ((z ^ (z >> 27)) * 10723151780598845931) & M64
        z = z ^ (z >> 31)
        if z == 0:
            z = 88172645463325252
        self.s = z

    def next(self):
        x = self.s
        x = x ^ (x >> 12)
        x = x ^ ((x << 25) & M64)
        x = x ^ (x >> 27)
        self.s = x
        return (x * 2685821657736338717) & M64

    def f64(self):
        return float(self.next() >> 11) * 1.1102230246251565e-16

    def sym(self):
        return self.f64() * 2.0 - 1.0


def noise_white(n, seed):
    r = Rng(seed)
    return [r.sym() for _ in range(n)]


def blep(t, dt):
    if t < dt:
        x = t / dt
        return x + x - x * x - 1.0
    if t > 1.0 - dt:
        x = (t - 1.0) / dt
        return x * x + x + x + 1.0
    return 0.0


def osc_saw(n, rate, freq):
    ph = fract(0.0)
    out = []
    for i in range(n):
        f = freq[i]
        dt = f / rate
        if dt < 0.0:
            dt = 0.0 - dt
        if dt > 0.49:
            dt = 0.49
        p = ph
        v = 2.0 * p - 1.0 - blep(p, dt)
        out.append(v)
        ph = ph + f / rate
        ph = ph - ffloor(ph)
    return out


# ------------------------------------------------------------------ synth_fx.fi
def clamp_cut(fc, rate):
    f = fc
    if f < 10.0:
        f = 10.0
    if f > rate * 0.45:
        f = rate * 0.45
    return f


def clamp_q(q):
    if q < 0.1:
        return 0.1
    return q


def fx_biquad_bp(inp, rate, cutoff, q):
    n = len(inp)
    z1 = z2 = 0.0
    b0 = 1.0
    b1 = b2 = a1 = a2 = 0.0
    out = []
    for i in range(n):
        if i % 8 == 0:
            w = clamp_cut(cutoff[i], rate) / rate
            sn = sin_turn(w)
            cs = cos_turn(w)
            alpha = sn / (2.0 * clamp_q(q[i]))
            c0 = alpha
            c1 = 0.0
            c2 = 0.0 - alpha
            a0 = 1.0 + alpha
            b0 = c0 / a0
            b1 = c1 / a0
            b2 = c2 / a0
            a1 = (0.0 - 2.0 * cs) / a0
            a2 = (1.0 - alpha) / a0
        x = inp[i]
        y = b0 * x + z1
        z1 = b1 * x - a1 * y + z2
        z2 = b2 * x - a2 * y
        out.append(y)
    return out


def to_s16(sig):
    out = bytearray()
    for x in sig:
        v = x * 32767.0
        if v >= 0.0:
            v = v + 0.5
        else:
            v = v - 0.5
        q = int(v)
        if q > 32767:
            q = 32767
        if q < -32767:
            q = -32767
        out += (q & 65535).to_bytes(2, "little")
    return bytes(out)


def crc(sig):
    return zlib.crc32(to_s16(sig)) & 0xFFFFFFFF


# ------------------------------------------------------------------ formant_table.fi
VF = {"ah": (730, 1090, 2440), "ee": (270, 2290, 3010), "oo": (300, 870, 2240), "ae": (660, 1720, 2410),
      "oh": (570, 840, 2410)}
GAIN = (1.0, 0.9, 0.7)
BANK_GAIN = 1.5


def vowel_f(v, k):
    return float(VF[v][k])


def vowel_bw(v, k):
    if k == 0:
        return 70.0 if v in ("ee", "oo") else 60.0
    return 90.0 if k == 1 else 150.0


# ------------------------------------------------------------------ formant.fi
def smoothstep(u):
    if u <= 0.0:
        return 0.0
    if u >= 1.0:
        return 1.0
    return u * u * (3.0 - 2.0 * u)


def vibrato_apply(f0, rate, vib_rate, cents, fade_ms):
    if cents == 0.0 or vib_rate <= 0.0:
        return f0
    vph = 0.0
    dph = vib_rate / rate
    out = []
    for i in range(len(f0)):
        d = 1.0
        if fade_ms > 0.0:
            d = (float(i) * 1000.0 / rate) / fade_ms
            if d > 1.0:
                d = 1.0
        e = cents * (1.0 / 1200.0) * d * sin_turn(vph)
        out.append(f0[i] * pow2(e))
        vph = vph + dph
        vph = vph - ffloor(vph)
    return out


def glottis(n, rate, kind, f0):
    if kind == 1:
        ph = 0.0
        out = []
        for i in range(n):
            f = f0[i]
            if f < 20.0:
                f = 20.0
            ph = ph + f / rate
            v = 0.0
            if ph >= 1.0:
                ph = ph - 1.0
                v = 0.06 * rate / f
            out.append(v)
        return out
    return osc_saw(n, rate, f0)


def track_morph(n, va, vb, t0, t1, shift):
    F = []
    BW = []
    for k in range(3):
        fa = vowel_f(va, k) * shift
        fb = vowel_f(vb, k) * shift
        ba = vowel_bw(va, k)
        bb = vowel_bw(vb, k)
        fl, bl = [], []
        for i in range(n):
            if i >= t1:
                u = 1.0
            elif i > t0:
                u = smoothstep(float(i - t0) / float(t1 - t0))
            else:
                u = 0.0
            fl.append(fa + (fb - fa) * u)
            bl.append(ba + (bb - ba) * u)
        F.append(fl)
        BW.append(bl)
    return F, BW


def formant_bank(src, rate, F, BW):
    n = len(src)
    out = [0.0] * n
    for k in range(3):
        q = []
        for i in range(n):
            b = BW[k][i]
            if b < 1.0:
                b = 1.0
            q.append(F[k][i] / b)
        tmp = fx_biquad_bp(src, rate, F[k], q)
        g = GAIN[k]
        for i in range(n):
            out[i] = out[i] + g * tmp[i]
    return out


def sing_morph(n, rate, f0, vib_rate, vib_cents, vib_fade, va, vb, t0_ms, t1_ms, shift, level, kind):
    fc = [f0] * n
    fc = vibrato_apply(fc, rate, vib_rate, vib_cents, vib_fade)
    src = glottis(n, rate, kind, fc)
    t0 = int(t0_ms * rate / 1000.0)
    t1 = int(t1_ms * rate / 1000.0)
    F, BW = track_morph(n, va, vb, t0, t1, shift)
    out = formant_bank(src, rate, F, BW)
    g = BANK_GAIN * level
    return [x * g for x in out]


# ------------------------------------------------------------------ formant_cons.fi
CONS = {  # center, q, gain
    "s": (7000.0, 1.6, 1.0), "sh": (3200.0, 1.8, 1.1), "f": (5500.0, 0.5, 0.35), "z": (7000.0, 1.6, 0.55),
    "b": (700.0, 0.8, 1.0), "d": (4500.0, 1.2, 1.0), "g": (1800.0, 1.5, 1.0), "p": (700.0, 0.8, 1.0),
    "t": (4500.0, 1.2, 1.0), "k": (1800.0, 1.5, 1.0),
}
THUMP = {"b": 110.0, "d": 150.0, "g": 130.0}


def band_noise(n, rate, center, q, seed):
    w = noise_white(n, seed)
    return fx_biquad_bp(w, rate, [center] * n, [q] * n)


def fric(n, rate, c, seed):
    cen, q, g = CONS[c]
    out = band_noise(n, rate, cen, q, seed)
    att = int(0.010 * rate)
    rel = int(0.015 * rate)
    if att > n // 3:
        att = n // 3
    if rel > n // 3:
        rel = n // 3
    for i in range(n):
        e = 1.0
        if i < att:
            e = float(i + 1) / float(att + 1)
        elif i + rel >= n:
            e = float(n - i) / float(rel + 1)
        out[i] = out[i] * g * e
    return out


def burst_len(rate):
    return (rate * 12 + 500) // 1000


def burst(rate, c, seed):
    ln = burst_len(rate)
    cen, q, g = CONS[c]
    voiced = c in ("b", "d", "g")
    noise = band_noise(ln, float(rate), cen, q, seed)
    if voiced:
        g = g * 0.5
    hz = THUMP.get(c, 0.0)
    ph = 0.0
    out = []
    for i in range(ln):
        u = float(i) / float(ln)
        e = (1.0 - u) * (1.0 - u)
        v = g * noise[i] * e
        if voiced:
            v = v + 0.7 * sin_turn(ph) * e
            ph = ph + hz / float(rate)
            ph = ph - ffloor(ph)
        out.append(v)
    return out


# ------------------------------------------------------------------ scenarios (must match tests/4962)
SCEN = [
    ("R1", lambda: sing_morph(4410, 22050.0, 150.0, 6.0, 40.0, 100.0, "ah", "ee", 50.0, 150.0, 1.0, 1.0, 0)),
    ("R2", lambda: sing_morph(4410, 22050.0, 180.0, 0.0, 0.0, 0.0, "oo", "oo", 0.0, 0.0, 1.17, 0.7, 1)),
    ("R3", lambda: fric(2205, 22050.0, "s", 5)),
    ("R4", lambda: fric(2205, 22050.0, "sh", 6)),
    ("R5", lambda: fric(1500, 22050.0, "f", 7)),
    ("R6", lambda: burst(22050, "g", 8)),
    ("R7", lambda: burst(22050, "k", 9)),
]

# independent check: one formant, F = 730 Hz, BW = 60 Hz, 2000 samples of white noise (seed 1), rate 22050
IND_IDX = list(range(100, 2000, 100))


def independent():
    n = 2000
    rate = 22050.0
    x = noise_white(n, 1)
    q = 730.0 / 60.0
    ref = fx_biquad_bp(x, rate, [730.0] * n, [q] * n)  # what Firn does (table sine)
    import numpy as np
    from scipy.signal import lfilter
    w0 = 2.0 * math.pi * 730.0 / rate
    alpha = math.sin(w0) / (2.0 * q)
    a0 = 1.0 + alpha
    b = [alpha / a0, 0.0, -alpha / a0]
    a = [1.0, -2.0 * math.cos(w0) / a0, (1.0 - alpha) / a0]
    y = lfilter(b, a, np.array(x))
    return x, ref, [float(v) for v in y]


def emit():
    lines = ["// REF-BEGIN (generated by tools/formant/gen_ref.py -- do not edit by hand)"]
    for name, fn in SCEN:
        sig = fn()
        lines.append("const %s_N: usize = %d" % (name, len(sig)))
        lines.append("const %s_CRC: u32 = %d as u32" % (name, crc(sig)))
    x, ref, y = independent()
    lines.append("fn ind_ref(i: usize) -> f64 {")
    for i in IND_IDX:
        lines.append("    if i == %d { return %s }" % (i, repr(y[i])))
    lines.append("    return 0.0")
    lines.append("}")
    lines.append("// REF-END")
    return "\n".join(lines) + "\n"


def main():
    args = sys.argv[1:]
    if "--report" in args:
        x, ref, y = independent()
        pk = max(abs(v) for v in y)
        dev = max(abs(ref[i] - y[i]) for i in range(len(y)))
        devi = max(abs(ref[i] - y[i]) for i in IND_IDX)
        print("independent: peak %.6g, max |firn_port - scipy| %.3g (%.3g of peak), at checkpoints %.3g" % (pk, dev, dev / pk, devi))
        return
    text = emit()
    if "--patch" in args or "--check" in args:
        flag = "--patch" if "--patch" in args else "--check"
        path = args[args.index(flag) + 1]
        src = open(path).read()
        new, cnt = re.subn(r"// REF-BEGIN.*?// REF-END\n", lambda m: text, src, flags=re.S)
        if cnt != 1:
            print("no REF block in", path)
            sys.exit(2)
        if flag == "--patch":
            open(path, "w").write(new)
            print("patched", path)
        elif new != src:
            print("REF block out of date:", path)
            sys.exit(1)
        return
    sys.stdout.write(text)


main()
