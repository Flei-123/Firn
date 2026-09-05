#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/paintb3/png_check.py -- the two directions of PNG, each checked
against somebody else.

  * the ENCODER: a page is rendered twice, once as PPM and once as PNG.
    **Pillow** reads the PNG, and its pixels have to be the PPM's pixels.
    Nothing here reads the file it wrote.
  * the DECODER: Pillow writes PNG files -- greyscale, RGB, RGBA, with
    every one of the five row filters forced in turn -- and
    `lib/paint/png_main.fi` reads them back. What it returns has to be
    what Pillow put in.

The five filters matter: PNG predicts every row from the row above and
from the pixel to the left, and an implementation that gets `Paeth` wrong
is right on four filters out of five and produces noise on the fifth.
Pillow is asked to use each of them.
"""
import argparse
import io
import os
import struct
import subprocess
import sys

import numpy as np
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def blk(b):
    return struct.pack("<I", len(b)) + b


def read_ppm(data):
    assert data[:2] == b"P6", data[:16]
    i, vals = 2, []
    while len(vals) < 3:
        while data[i:i + 1].isspace():
            i += 1
        j = i
        while not data[j:j + 1].isspace():
            j += 1
        vals.append(int(data[i:j]))
        i = j
    i += 1
    w, h, _ = vals
    return np.frombuffer(data[i:i + w * h * 3], dtype=np.uint8).reshape(h, w, 3)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("paintb3")
    ap.add_argument("pngb3")
    args = ap.parse_args()

    ua = open(os.path.join(ROOT, "lib/dom/ua.css"), "rb").read()
    font = open(os.path.join(ROOT, "tests/data/fonts/FirnSans.ttf"), "rb").read()
    html = (b"<!doctype html><html><body>"
            b"<div style='background:#3366cc;width:120px;height:70px;"
            b"border-radius:14px;border:5px solid #cc3333'></div>"
            b"<div style='background:linear-gradient(60deg,#ff0000,#00ff40);"
            b"width:160px;height:50px'></div>"
            b"<p style='color:#101080;font-size:19px'>PNG, both ways.</p>"
            b"</body></html>")
    bad = 0

    # ---- the encoder, against Pillow
    for flags, name in ((0, "ppm"), (1, "png")):
        j = (struct.pack("<II", 240, 200) + blk(html) + blk(ua) + blk(b"")
             + blk(font) + struct.pack("<I", flags))
        p = subprocess.run([args.paintb3], input=j, capture_output=True)
        if p.returncode != 0:
            print("   FAILED: paintb3 returned %d" % p.returncode)
            return 1
        if name == "ppm":
            ppm = read_ppm(p.stdout)
        else:
            pngbytes = p.stdout
    im = Image.open(io.BytesIO(pngbytes))
    arr = np.array(im.convert("RGBA"))
    # The PPM put the picture on white; do the same with the PNG's alpha.
    a = arr[:, :, 3:4].astype(np.int32)
    rgb = ((arr[:, :, :3].astype(np.int32) * a + 255 * (255 - a)) // 255)
    d = np.abs(rgb - ppm.astype(np.int32))
    print("   encoder       %dx%d, %s, largest deviation from the PPM: %d"
          % (im.size[0], im.size[1], im.mode, int(d.max())))
    if int(d.max()) > 1:
        print("   FAILED: the PNG this engine wrote is not the picture it drew")
        bad += 1

    # ---- the decoder, against Pillow, once per row filter and colour type
    rng = np.random.default_rng(7)
    cases = []
    for mode in ("L", "RGB", "RGBA", "LA"):
        base = rng.integers(0, 256, size=(37, 53, len(mode)), dtype=np.uint8)
        # a smooth part, so that the filters have something to predict
        yy, xx = np.mgrid[0:37, 0:53]
        base[:, :, 0] = ((xx * 3 + yy * 5) % 256).astype(np.uint8)
        cases.append((mode, Image.fromarray(base.squeeze(), mode)))
    worst = 0
    for mode, im in cases:
        for filt in range(5):
            buf = io.BytesIO()
            im.save(buf, format="PNG", compress_level=6, filter=filt)
            data = buf.getvalue()
            p = subprocess.run([args.pngb3], input=data, capture_output=True)
            if p.returncode != 0:
                print("   FAILED: pngb3 refused a %s PNG with filter %d"
                      % (mode, filt))
                bad += 1
                continue
            got = read_ppm(p.stdout)
            ref = np.array(im.convert("RGBA")).astype(np.int32)
            a = ref[:, :, 3:4]
            want = (ref[:, :, :3] * a + 255 * (255 - a)) // 255
            d = int(np.abs(got.astype(np.int32) - want).max())
            worst = max(worst, d)
            if d > 1:
                print("   FAILED: %s PNG, filter %d: deviation %d"
                      % (mode, filt, d))
                bad += 1
    print("   decoder       4 colour types x 5 row filters read back, "
          "largest deviation: %d" % worst)
    print("PNG: %s" % ("OK" if bad == 0 else "%d FAILURES" % bad))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
