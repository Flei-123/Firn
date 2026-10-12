#!/usr/bin/env python3
"""tools/gpu/fuigl/cmp.py -- compares what the X server shows for the CPU path and the GL path of fUi.

usage: cmp.py <cpu dir> <gl dir> <w> <h> [min psnr] [file suffix, e.g. 2 = shot2.xwd / canvas2.rgba]
Each dir holds shot.xwd (xwd -root) and canvas.rgba (RGBA canvas written by fuigl). The window sits at 0,0 (no window
manager). Checks: (1) CPU screenshot == its canvas (sanity of the harness), (2) GL screenshot vs its canvas, (3) GL
screenshot vs CPU screenshot: differing pixels, max channel diff, PSNR (RGB). Exit 0 if (3) PSNR >= min (default 34).
"""
import math
import struct
import sys


def read_xwd(path, w, h):
    d = open(path, "rb").read()
    hdr = struct.unpack(">25I", d[:100])
    hsize, ww, hh, bpp, bpl = hdr[0], hdr[4], hdr[5], hdr[11], hdr[12]
    ncolors = hdr[19]
    off = hsize + ncolors * 12
    assert bpp == 32, bpp
    px = d[off:]
    out = bytearray(w * h * 3)
    for y in range(h):
        row = px[y * bpl:y * bpl + w * 4]
        for x in range(w):
            b, g, r = row[x * 4], row[x * 4 + 1], row[x * 4 + 2]  # little-endian BGRX
            o = (y * w + x) * 3
            out[o], out[o + 1], out[o + 2] = r, g, b
    return bytes(out)


def read_canvas(path, w, h):
    d = open(path, "rb").read()
    assert len(d) == w * h * 4, (len(d), w, h)
    out = bytearray(w * h * 3)
    out[0::3] = d[0::4]
    out[1::3] = d[1::4]
    out[2::3] = d[2::4]
    return bytes(out)


def diff(a, b):
    n = 0
    mx = 0
    sq = 0
    for i in range(0, len(a), 3):
        if a[i:i + 3] != b[i:i + 3]:
            n += 1
        for k in range(3):
            e = abs(a[i + k] - b[i + k])
            mx = max(mx, e)
            sq += e * e
    mse = sq / len(a)
    psnr = 99.0 if mse == 0 else 10 * math.log10(255 * 255 / mse)
    return n, mx, psnr


def main():
    cpu_dir, gl_dir, w, h = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    need = float(sys.argv[5]) if len(sys.argv) > 5 else 34.0
    sfx = sys.argv[6] if len(sys.argv) > 6 else ""
    cs = read_xwd(cpu_dir + "/shot" + sfx + ".xwd", w, h)
    gs = read_xwd(gl_dir + "/shot" + sfx + ".xwd", w, h)
    cc = read_canvas(cpu_dir + "/canvas" + sfx + ".rgba", w, h)
    gc = read_canvas(gl_dir + "/canvas" + sfx + ".rgba", w, h)
    fails = 0
    n, mx, p = diff(cs, cc)
    print(f"   cpu screen vs cpu canvas: diff_px={n} max={mx} psnr={p:.2f}")
    if n:
        fails += 1
    n, mx, p = diff(gs, gc)
    print(f"   gl screen vs gl canvas:   diff_px={n} max={mx} psnr={p:.2f}")
    if n:
        fails += 1
    n, mx, p = diff(gs, cs)
    print(f"   gl screen vs cpu screen:  diff_px={n} max={mx} psnr={p:.2f} (need >= {need})")
    if p < need:
        fails += 1
    # the picture must not be empty (all one colour would pass a lazy compare)
    colours = len({gs[i:i + 3] for i in range(0, len(gs), 3 * 7)})
    print(f"   gl screen colours={colours}")
    if colours < 4:
        print("   FAIL: GL screenshot is (nearly) one colour")
        fails += 1
    print("RESULT fuigl cmp: " + ("ok" if fails == 0 else "FAIL"))
    return 1 if fails else 0


sys.exit(main())
