#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import subprocess
import sys


def render(exe, fixture, page):
    out = subprocess.run([exe, "r", fixture, str(page), "1"], capture_output=True, check=True).stdout
    nl = out.index(b"\n")
    w, h = map(int, out[:nl].split())
    return w, h, out[nl + 1:nl + 1 + w * h * 4]


def at(img, x, y):
    w, _, px = img
    i = (y * w + x) * 4
    return tuple(px[i:i + 4])


def region(img, x0, y0, x1, y1):
    return {at(img, x, y) for y in range(y0, y1) for x in range(x0, x1)}


def main():
    exe, fixture = sys.argv[1], sys.argv[2]
    img = render(exe, fixture, 1)
    fails = []
    seen = region(img, 0, 0, img[0], img[1])
    if (200, 30, 30, 255) not in seen:
        fails.append("picture colour not RGB-exact")
    if (30, 30, 200, 255) in seen:
        fails.append("red/blue swapped")
    crop = region(img, 560, 340, 678, 426)
    if (20, 200, 20, 255) not in crop or (10, 20, 230, 255) in crop:
        fails.append("srcRect crop")
    if at(img, 684, 170) != (255, 255, 255, 255):
        fails.append("ellipse corner not clear")
    cx = at(img, 787, 220)
    if not (100 <= cx[0] <= 160 and cx[1] < 20 and 100 <= cx[2] <= 160):
        fails.append("ellipse gradient centre %r" % (cx,))
    left = at(img, 700, 220)
    right = at(img, 875, 220)
    if not (left[0] > right[0] and left[2] < right[2]):
        fails.append("gradient direction")
    if at(img, 684, 306) != (255, 255, 255, 255):
        fails.append("round corner not clear")
    if at(img, 787, 351) != (0, 170, 0, 255):
        fails.append("round fill %r" % (at(img, 787, 351),))
    if at(img, 787, 305) != (0, 0, 0, 255):
        fails.append("round outline %r" % (at(img, 787, 305),))
    if fails:
        print("FAIL " + "; ".join(fails))
        return 1
    print("OK pptx pixels")
    return 0


sys.exit(main())
