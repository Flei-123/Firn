#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/av1/mkfixtures.py <outdir> -- small AVIF fixtures made with Pillow (libavif): sizes, 4:2:0/4:4:4/4:0:0, alpha, qualities."""
import os, sys, random
from PIL import Image, ImageDraw
out = sys.argv[1]
os.makedirs(out, exist_ok=True)
random.seed(7)
def pic(w, h, mode="RGB"):
    im = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(im)
    for y in range(h):
        d.line([(0, y), (w, y)], fill=(y * 255 // max(h, 1), (y * 7) % 256, 255 - y * 255 // max(h, 1)))
    for _ in range(12):
        x, y = random.randrange(w), random.randrange(h)
        d.ellipse([x, y, x + random.randrange(4, 40), y + random.randrange(4, 40)], fill=tuple(random.randrange(256) for _ in range(3)))
    d.text((3, 3), "avif", fill=(255, 255, 255))
    return im
cases = [("s16_420", 16, 16, "4:2:0", 80, False), ("s64_420", 64, 48, "4:2:0", 60, False), ("s37_420", 37, 23, "4:2:0", 75, False),
         ("s64_444", 64, 48, "4:4:4", 80, False), ("s50_422", 50, 30, "4:2:2", 80, False), ("s64_400", 64, 40, "4:0:0", 80, False),
         ("s64_420a", 64, 48, "4:2:0", 70, True), ("t300_tiles", 300, 200, "4:2:0", 60, False, dict(tile_rows=2, tile_cols=2)),
         ("t520_lr", 520, 360, "4:2:0", 30, False, dict(speed=2)), ("t256_444lr", 256, 256, "4:4:4", 40, False, dict(speed=1)),
         ("t130_tiles", 130, 300, "4:2:0", 75, False, dict(tile_rows=4, tile_cols=1)), ("s200_420", 200, 120, "4:2:0", 50, False), ("s1_1", 1, 1, "4:2:0", 80, False),
         ("s300_444q100", 300, 200, "4:4:4", 100, False), ("s129_420", 129, 131, "4:2:0", 35, False)]
for case in cases:
    name, w, h, ss, q, alpha = case[:6]
    extra = case[6] if len(case) > 6 else {}
    im = pic(w, h)
    if alpha:
        im = im.convert("RGBA")
        im.putalpha(Image.linear_gradient("L").resize((w, h)))
    try:
        im.save(os.path.join(out, name + ".avif"), **dict(dict(quality=q, subsampling=ss, speed=6), **extra))
    except Exception as e:
        print("skip", name, e)
