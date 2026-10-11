#!/usr/bin/env python3
"""tools/mc/sky/ref.py -- independent reference for lib/mc/mc_sky.fi.

Reads <dir>/scenes.txt (written by dump.fi), recomputes the sky state from (ticks, rain) and renders every scene
pixel by pixel with its own code (python floats = IEEE f64, exact python integers, no libm), then compares the
state fields, the f64 bit patterns of the sun direction, the framebuffer <dir>/<scene>.rgba byte for byte and the
SHA-256. Written from docs/mc/sky.md. Usage: ref.py <dir>   (prints one line per scene, exit 1 on any difference)
"""
import hashlib
import math
import struct
import sys

OFF = 1000000


def f64(h):
    return struct.unpack("<d", struct.pack("<Q", int(h, 16)))[0]


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def lerp(a, b, t):
    return (a * (256 - t) + b * t) // 256


def clamp01(x):
    return 0.0 if x < 0.0 else (1.0 if x > 1.0 else x)


def bsin(x):
    a, sign = x, 1.0
    if a > 180.0:
        a, sign = a - 180.0, -1.0
    p = a * (180.0 - a)
    return sign * ((4.0 * p) / (40500.0 - p))


def state(ticks, rain):
    t = ticks % 24000  # python % is non-negative for a positive modulus
    rn = max(0, min(256, rain))
    ang = (float(t) * 360.0) / 24000.0
    sn = bsin(ang)
    c = ang + 90.0
    if c >= 360.0:
        c -= 360.0
    cs = bsin(c)
    sun = (cs, sn * 0.9, sn * 0.4358898943540674)
    d = clamp01((sun[1] + 0.1) / 0.3)
    dl = math.floor(d * 256.0)
    a = 1.0 - abs(sun[1]) / 0.35
    a = max(a, 0.0)
    gl = math.floor((256.0 * a) * a) * (256 - rn) // 256
    gy = lerp(14, 150, dl)
    gz = gy * 3 // 4
    zen = [lerp(3, 82, dl), lerp(4, 131, dl), lerp(14, 235, dl)]
    hor = [lerp(10, 190, dl), lerp(12, 214, dl), lerp(30, 255, dl)]
    zen = [lerp(zen[0], gz, rn), lerp(zen[1], gz + 1, rn), lerp(zen[2], gz + 4, rn)]
    hor = [lerp(hor[0], gy, rn), lerp(hor[1], gy + 2, rn), lerp(hor[2], gy + 6, rn)]
    tint = [lerp(96, 256, dl), lerp(110, 256, dl), lerp(190, 256, dl)]
    wg = gl // 2
    tint = [lerp(tint[0], 256, wg), lerp(tint[1], 224, wg), lerp(tint[2], 184, wg)]
    rh = rn // 2
    tint = [lerp(tint[0], 216, rh), lerp(tint[1], 220, rh), lerp(tint[2], 232, rh)]
    return dict(time=t, rain=rn, day=dl, glow=gl, discs=1 if rn < 128 else 0, sun=sun, zen=zen, hor=hor, tint=tint)


def render(w, h, cam, st, tint):
    rx, ry, rz, ux, uy, uz, fx, fy, fz, tan_x, tan_y = cam
    hw, hh = w * 0.5, h * 0.5
    fpx, fpy = hw / tan_x, hh / tan_y
    sun = st["sun"]
    glowf = float(st["glow"])
    out = bytearray(w * h * 4)
    for y in range(h):
        ey = (hh - (y + 0.5)) / fpy
        for x in range(w):
            ex = ((x + 0.5) - hw) / fpx
            dx = (fx + ex * rx) + ey * ux
            dy = (fy + ex * ry) + ey * uy
            dz = (fz + ex * rz) + ey * uz
            ln = math.sqrt((dx * dx + dy * dy) + dz * dz)
            nx, ny, nz = dx / ln, dy / ln, dz / ln
            g = clamp01(ny * 1.6)
            gg = 1.0 - (1.0 - g) * (1.0 - g)
            f = math.floor(gg * 256.0)
            col = [lerp(st["hor"][i], st["zen"][i], f) for i in range(3)]
            dsun = (nx * sun[0] + ny * sun[1]) + nz * sun[2]
            if dsun > 0.0 and glowf > 0.0:
                d3 = (dsun * dsun) * dsun
                fg = math.floor(d3 * (1.0 - gg) * glowf)
                col = [lerp(col[0], 255, fg), lerp(col[1], 120, fg), lerp(col[2], 60, fg)]
            if st["discs"]:
                if dsun >= 0.9976:
                    col = [255, 248, 200]
                elif (0.0 - dsun) >= 0.9984:
                    col = [226, 228, 240]
            if tint and (x + y) % 3 == 0:
                col = [(col[i] * st["tint"][i]) // 256 for i in range(3)]
            o = (y * w + x) * 4
            out[o], out[o + 1], out[o + 2], out[o + 3] = col[0], col[1], col[2], 255
    return bytes(out)


def main(d):
    lines = [l.split() for l in open(d + "/scenes.txt") if l.strip()]
    bad = 0
    i = 0
    n = 0
    while i < len(lines):
        sc, cam, sk, hs = lines[i], lines[i + 1], lines[i + 2], lines[i + 3]
        i += 4
        name, w, h = sc[1], int(sc[2]), int(sc[3])
        ticks, rain, tint = int(sc[4]) - OFF, int(sc[5]) - OFF, sc[6] == "1"
        camv = [f64(x) for x in cam[1:]]
        st = state(ticks, rain)
        problems = []
        v = [int(x) for x in sk[1:6]]
        if [v[0] - OFF, v[1] - OFF, v[2] - OFF, v[3] - OFF, v[4]] != [st["time"], st["rain"], st["day"], st["glow"], st["discs"]]:
            problems.append("state head %s != %s" % (v, [st["time"], st["rain"], st["day"], st["glow"], st["discs"]]))
        if [int(x, 16) for x in sk[6:9]] != [bits(c) for c in st["sun"]]:
            problems.append("sun direction bits differ")
        cols = [int(x) - OFF for x in sk[9:24]]
        if cols != st["zen"] + st["hor"] + st["hor"] + st["tint"]:
            problems.append("colours %s != %s" % (cols, st["zen"] + st["hor"] + st["hor"] + st["tint"]))
        img = render(w, h, camv, st, tint)
        fire = open("%s/%s.rgba" % (d, name), "rb").read()
        if img != fire:
            cnt = sum(1 for k in range(0, len(img), 4) if img[k:k + 4] != fire[k:k + 4])
            problems.append("framebuffer differs in %d pixels" % cnt)
        if hashlib.sha256(img).hexdigest() != hs[1] and not problems:
            problems.append("hash differs")
        if problems:
            bad += 1
            print("  DIFF  %s: %s" % (name, "; ".join(problems)))
        else:
            print("  ok    %s (%dx%d t=%d rain=%d%s) %s" % (name, w, h, ticks, rain, " tint" if tint else "", hs[1][:12]))
        n += 1
    # invariants of the model that do not depend on the Firn output
    day = [state(t, 0)["day"] for t in range(0, 24000, 250)]
    if not (state(6000, 0)["day"] == 256 and state(18000, 0)["day"] == 0 and max(day) == 256 and min(day) == 0):
        bad += 1
        print("  DIFF  invariant: noon must be full day, midnight dark")
    if not (state(0, 0)["glow"] == 256 and state(12000, 0)["glow"] == 256 and state(6000, 0)["glow"] == 0):
        bad += 1
        print("  DIFF  invariant: glow only at sunrise / sunset")
    if state(30000, 5)["sun"] != state(6000, 5)["sun"]:
        bad += 1
        print("  DIFF  invariant: tick wrap")
    # the Bhaskara sine is within 0.0017 of the real one
    err = max(abs(bsin(a * 0.5) - math.sin(math.radians(a * 0.5))) for a in range(0, 719))
    if err > 0.0017:
        bad += 1
        print("  DIFF  sine error %g" % err)
    else:
        print("  ok    invariants (day/night/glow/wrap), sine error %.5f" % err)
    print("  %d scenes, %d problems" % (n, bad))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
