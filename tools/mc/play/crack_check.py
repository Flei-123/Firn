#!/usr/bin/env python3
import sys

from PIL import Image


def lum(im):
    return im.convert("L")


def darker(base, img, thr):
    a = lum(base).load()
    b = lum(img).load()
    w, h = base.size
    n = 0
    for y in range(h):
        for x in range(w):
            if a[x, y] - b[x, y] > thr:
                n += 1
    return n


def differing(a_im, b_im):
    a = a_im.convert("RGB").load()
    b = b_im.convert("RGB").load()
    w, h = a_im.size
    n = 0
    for y in range(h):
        for x in range(w):
            if a[x, y] != b[x, y]:
                n += 1
    return n


def main():
    mode = sys.argv[1]
    before, s1, s2, after = [Image.open(p) for p in sys.argv[2:6]]
    d1 = darker(before, s1, 25)
    d2 = darker(before, s2, 25)
    resid = differing(before, after)
    print(f"  {mode}: dark pixels early={d1} late={d2} residual_after_cancel={resid}")
    ok = True
    if d1 < 40:
        print(f"  FAIL  {mode}: no crack visible early")
        ok = False
    if d2 < 150 or d2 < d1 * 1.3:
        print(f"  FAIL  {mode}: crack does not grow (early {d1}, late {d2})")
        ok = False
    if resid > 30:
        print(f"  FAIL  {mode}: crack still visible after cancel ({resid} pixels differ)")
        ok = False
    sys.exit(0 if ok else 1)


main()
