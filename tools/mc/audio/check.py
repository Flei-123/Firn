#!/usr/bin/env python3
"""tools/mc/audio/check.py -- reference for the pure spatial math of lib/mc/mc_audio.fi.

Reads the text of tools/mc/audio/dump.fi (floats as the hex of their f32 bits) and recomputes
every result with numpy float32 scalars, in the same order of operations as the Aulos engine
(engine.compute_targets). Attenuation, pan and Minecraft gain must be BIT IDENTICAL. The
cos / sin of the pan law are f64 libm values rounded to f32 (engine E5: std.math is f64); they
are counted exactly and may differ by at most 1 ulp of f32.

usage: check.py <dump-file>
"""
import math
import struct
import sys

import numpy as np

F = np.float32


def bits(x):
    return struct.unpack("<I", struct.pack("<f", float(x)))[0]


def val(h):
    return F(struct.unpack("<f", struct.pack("<I", int(h, 16)))[0])


def clampf(v, lo, hi):
    if v < lo:
        return lo
    if v > hi:
        return hi
    return v


def vlen(x, y, z):
    return F(np.sqrt(F(F(F(x * x) + F(y * y)) + F(z * z))))


def vnorm(x, y, z):
    l = vlen(x, y, z)
    if l < F(0.000001):
        return F(0), F(0), F(0)
    return F(x / l), F(y / l), F(z / l)


def atten(d, mn, mx, linear, ro):
    d = clampf(d, mn, mx)
    if linear:
        span = F(mx - mn)
        if span <= F(0.000001):
            a = F(1.0)
        else:
            a = F(F(mx - d) / span)
    else:
        a = F(mn / F(mn + F(ro * F(d - mn))))
    return clampf(a, F(0.0), F(1.0))


def pan_of(rx, ry, rz, f, u):
    dist = vlen(rx, ry, rz)
    fx, fy, fz = vnorm(*f)
    ux, uy, uz = vnorm(*u)
    cx = F(F(fy * uz) - F(fz * uy))
    cy = F(F(fz * ux) - F(fx * uz))
    cz = F(F(fx * uy) - F(fy * ux))
    gx, gy, gz = vnorm(cx, cy, cz)
    if dist > F(0.00001):
        dot = F(F(F(rx * gx) + F(ry * gy)) + F(rz * gz))
        return clampf(F(dot / dist), F(-1.0), F(1.0))
    return F(0.0)


def pan_lr(pan):
    pi = F(3.14159265358979323846)
    angle = F(F(pan + F(1.0)) * F(pi * F(0.25)))
    return F(math.cos(float(angle))), F(math.sin(float(angle)))


def ulp_diff(a, b):
    ia, ib = bits(a), bits(b)
    # same sign assumed for the magnitudes that matter; compare as ordered ints
    def key(i):
        return i if i < 0x80000000 else -(i & 0x7FFFFFFF)
    return abs(key(ia) - key(ib))


def main():
    lines = [l.split() for l in open(sys.argv[1]).read().splitlines() if l.strip()]
    bad = 0
    n = {"A": 0, "G": 0, "P": 0}
    trig_exact = 0
    trig_total = 0
    trig_max = 0
    for t in lines:
        k = t[0]
        if k == "A":
            d, mn, mx = val(t[1]), val(t[2]), val(t[3])
            lin = t[4] == "1"
            ro = val(t[5])
            want = atten(d, mn, mx, lin, ro)
            if bits(want) != int(t[6], 16):
                bad += 1
                print("FAIL atten", t, hex(bits(want)))
        elif k == "G":
            v, d, r = val(t[1]), val(t[2]), val(t[3])
            want = F(clampf(v, F(0.0), F(1.0)) * atten(d, F(1.0), r, True, F(1.0)))
            if bits(want) != int(t[4], 16):
                bad += 1
                print("FAIL gain", t, hex(bits(want)))
        elif k == "P":
            x = [val(h) for h in t[1:10]]
            pan = pan_of(x[0], x[1], x[2], x[3:6], x[6:9])
            if bits(pan) != int(t[10], 16):
                bad += 1
                print("FAIL pan", t, hex(bits(pan)))
                continue
            l, r = pan_lr(val(t[10]))
            for want, got in ((l, val(t[11])), (r, val(t[12]))):
                trig_total += 1
                u = ulp_diff(want, got)
                if u == 0:
                    trig_exact += 1
                trig_max = max(trig_max, u)
                if u > 1:
                    bad += 1
                    print("FAIL trig", t, float(want), float(got))
        else:
            bad += 1
            print("FAIL unknown line", t)
            continue
        n[k] += 1
    print(f"cases: atten {n['A']}, mc gain {n['G']}, pan {n['P']}; trig (cos/sin) exact {trig_exact}/{trig_total}, max {trig_max} ulp")
    return 1 if bad else 0


sys.exit(main())
