#!/usr/bin/env python3
"""ocr_words.py <png> <regex> [nth] -- print "x y" (centre, pixels) of the nth OCR word matching the regex, else nothing.
Pass 1: plain tesseract. Pass 2 (button labels, dark text on a bright coloured fill that tesseract skips): crop every large
bright-blue block, binarise (dark text -> black, fill -> white), read it with --psm 7/11, centre = centre of the block."""
import re
import subprocess
import sys
import tempfile
import os

from PIL import Image


def tsv(png, psm="11"):
    out = subprocess.run(["tesseract", png, "stdout", "--psm", psm, "tsv"], capture_output=True, text=True).stdout.splitlines()
    for ln in out[1:]:
        f = ln.split("\t")
        if len(f) >= 12 and f[11].strip():
            yield f[11].strip(), int(f[6]), int(f[7]), int(f[8]), int(f[9])


def blue_blocks(im):
    """bounding boxes of big blocks of the accent-blue button fill (rows with many blue pixels, grouped)"""
    rgb = im.convert("RGB")
    w, h = rgb.size
    px = rgb.load()
    rows = []
    for y in range(0, h, 2):
        xs = [x for x in range(0, w, 3) if px[x, y][2] > 200 and px[x, y][0] < 120 and px[x, y][1] > 90]
        rows.append((y, min(xs), max(xs), len(xs)) if len(xs) > w // 3 // 3 else None)
    blocks, cur = [], None
    for r in rows:
        if r is None:
            if cur:
                blocks.append(cur)
                cur = None
            continue
        if cur is None:
            cur = [r[1], r[0], r[2], r[0]]
        else:
            cur[0] = min(cur[0], r[1])
            cur[2] = max(cur[2], r[2])
            cur[3] = r[0]
    if cur:
        blocks.append(cur)
    return [b for b in blocks if b[3] - b[1] > 20]


def main():
    png, rx = sys.argv[1], sys.argv[2]
    nth = int(sys.argv[3]) if len(sys.argv) > 3 else 1
    hit = 0
    for word, x, y, w, h in tsv(png):
        if re.fullmatch(rx, word):
            hit += 1
            if hit == nth:
                print(x + w // 2, y + h // 2)
                return
    im = Image.open(png)
    with tempfile.TemporaryDirectory() as d:
        for i, (x0, y0, x1, y1) in enumerate(blue_blocks(im)):
            c = im.convert("L").crop((x0, y0, x1, y1)).point(lambda v: 0 if v < 70 else 255)
            c = c.resize((c.width * 2, c.height * 2))
            p = os.path.join(d, "b%d.png" % i)
            c.save(p)
            for psm in ("7", "11"):
                words = [t for t in tsv(p, psm)]
                if any(re.fullmatch(rx, t[0]) for t in words):
                    print((x0 + x1) // 2, (y0 + y1) // 2)
                    return


main()
