#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/glass_ref.py -- THE GLASS BLUR AGAINST A REFERENCE GAUSSIAN.
#
#     glass_ref.py <dir> [--max-box PCT] [--max-pipe PCT]
#
# <dir> holds src/box/gauss/pipe PNGs written by `glass_main blur <dir>`.
# The reference is a true Gaussian (numpy, separable, edge clamp) and
# Pillow's GaussianBlur. Printed per image: mean abs error in % of 255
# and the worst octet. Exit 1 if the box blur or the pipeline is worse
# than the limits.
import os
import sys

import numpy as np
from PIL import Image, ImageFilter


def load(d, n):
    return np.asarray(Image.open(os.path.join(d, n)).convert("RGB"), dtype=np.float64)


def gauss(img, sigma):
    r = int(np.ceil(sigma * 3))
    k = np.exp(-(np.arange(-r, r + 1) ** 2) / (2 * sigma * sigma))
    k /= k.sum()
    out = img
    for ax in (0, 1):
        pad = [(0, 0)] * 3
        pad[ax] = (r, r)
        p = np.pad(out, pad, mode="edge")
        acc = np.zeros_like(out)
        for i, w in enumerate(k):
            sl = [slice(None)] * 3
            sl[ax] = slice(i, i + out.shape[ax])
            acc += w * p[tuple(sl)]
        out = acc
    return out


def stat(a, b):
    d = np.abs(a - b)
    return d.mean() / 255.0 * 100.0, d.max()


def main():
    d = sys.argv[1]
    mb = 1.5
    mp = 2.5
    if "--max-box" in sys.argv:
        mb = float(sys.argv[sys.argv.index("--max-box") + 1])
    if "--max-pipe" in sys.argv:
        mp = float(sys.argv[sys.argv.index("--max-pipe") + 1])
    src = load(d, "src.png")
    box = load(d, "box.png")
    gau = load(d, "gauss.png")
    pipe = load(d, "pipe.png")
    s_box = (3 * ((2 * 3 + 1) ** 2 - 1) / 12.0) ** 0.5  # 3.464
    ref = gauss(src, s_box)
    pil = np.asarray(Image.fromarray(src.astype(np.uint8)).filter(ImageFilter.GaussianBlur(s_box)), dtype=np.float64)
    ident = stat(src, ref)
    print("GLASSREF unblurred vs gaussian: mean %.2f%% max %d (how much the blur matters)" % ident)
    m1 = stat(box, ref)
    print("GLASSREF box3 r=3 (sigma %.3f) vs numpy gaussian: mean %.3f%% max %d" % ((s_box,) + m1))
    m2 = stat(box, pil)
    print("GLASSREF box3 r=3 vs Pillow GaussianBlur: mean %.3f%% max %d" % m2)
    m3 = stat(gau, ref)
    print("GLASSREF blur_gauss vs numpy gaussian: mean %.3f%% max %d" % m3)
    # the pipeline: flog 1, rs 2 -> sigma_small = sqrt(3*(25-1)/12) = 2.449; full = x2,
    # plus the 2x2 mean (variance (4-1)/12) -> about 4.95
    sp = ((2 * 2.449489) ** 2 + 3.0 / 12.0) ** 0.5
    ref2 = gauss(src, sp)
    sl = (slice(32, src.shape[0] - 32), slice(32, src.shape[1] - 32))
    m4 = stat(pipe[sl], ref2[sl])
    print("GLASSREF pipeline (f=2, rs=2, sigma %.3f) vs numpy gaussian: mean %.3f%% max %d" % ((sp,) + m4))
    pil2 = np.asarray(Image.fromarray(src.astype(np.uint8)).filter(ImageFilter.GaussianBlur(sp)), dtype=np.float64)
    m5 = stat(pipe[sl], pil2[sl])
    print("GLASSREF pipeline vs Pillow GaussianBlur: mean %.3f%% max %d" % m5)
    bad = (m1[0] > mb) or (m2[0] > mb) or (m3[0] > 0.5) or (m4[0] > mp)
    print("GLASSREF %s" % ("FAIL" if bad else "OK"))
    sys.exit(1 if bad else 0)


main()
