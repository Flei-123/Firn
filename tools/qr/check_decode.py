#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/qr/check_decode.py -- holds the Firn QR decoder (lib/qr/qrdec.fi) against
# codes drawn by two implementations nobody here wrote (python-qrcode and
# segno) and against ZXing-C++ as the yardstick, on pictures that get worse:
#   clean      every version family, level, 2..12 pixels per module, quiet zone 4 and 2
#   rotated    any angle (bicubic, white fill)
#   warped     perspective, corners moved by up to 12 % of the side
#   blurred    Gaussian blur and noise, reduced contrast, an illumination ramp
#   damaged    a block of modules painted over (within what the level can repair)
#   cluttered  the code small inside a larger picture of lines and noise
# Every payload read must equal the payload that was encoded. ZXing-C++ reads the
# very same pictures; both counts are printed, and the run fails if ours is
# below the floor given for the category (floors are set from measured runs,
# see tools/qr/run.sh).
# usage: check_decode.py <qrdeccli binary> [seed] [per-category count] [--floor]
import math
import os
import random
import subprocess
import sys
import tempfile

import numpy as np
import qrcode
import segno
import zxingcpp
from PIL import Image, ImageDraw, ImageFilter
from qrcode import constants as qc

cli = sys.argv[1]
seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
per = int(sys.argv[3]) if len(sys.argv) > 3 else 40
rnd = random.Random(seed)
nprng = np.random.default_rng(seed)

QL = [qc.ERROR_CORRECT_L, qc.ERROR_CORRECT_M, qc.ERROR_CORRECT_Q, qc.ERROR_CORRECT_H]


def payload(maxlen):
    kind = rnd.random()
    n = rnd.randint(1, maxlen)
    if kind < 0.3:
        return "".join(rnd.choice("0123456789") for _ in range(n)).encode()
    if kind < 0.55:
        return "".join(rnd.choice("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 $%*+-./:") for _ in range(n)).encode()
    if kind < 0.85:
        words = ["https://example.org/", "login", "token=", "firn", "fUi", "wifi:T:WPA;S:", "id", "x"]
        s = ""
        while len(s) < n:
            s += rnd.choice(words) + str(rnd.randrange(1000))
        return s[:n].encode()
    return bytes(rnd.randrange(256) for _ in range(n))


def make_matrix(data, ecc, version=None):
    """Returns a numpy bool matrix (True = dark) from one of the two encoders."""
    if rnd.random() < 0.5:
        q = qrcode.QRCode(version=version, error_correction=QL[ecc], border=0)
        from qrcode.util import QRData
        q.add_data(QRData(data), optimize=0)
        q.make(fit=version is None)
        return np.array(q.modules, dtype=bool)
    sq = segno.make(data, error="LMQH"[ecc], micro=False, version=version, boost_error=False)
    return np.array([[bool(c) for c in r] for r in sq.matrix], dtype=bool)


def draw(m, scale, border):
    n = m.shape[0]
    side = (n + 2 * border) * scale
    a = np.full((side, side), 255, dtype=np.uint8)
    big = np.kron(m, np.ones((scale, scale), dtype=bool))
    a[border * scale:border * scale + n * scale, border * scale:border * scale + n * scale][big] = 0
    return Image.fromarray(a, "L")


def rotate(img, deg):
    return img.rotate(deg, resample=Image.BICUBIC, expand=True, fillcolor=255)


def warp(img, frac):
    w, h = img.size
    j = lambda: rnd.uniform(-frac, frac)
    src = [(0, 0), (w, 0), (w, h), (0, h)]
    dst = [(w * max(0, j()), h * max(0, j())), (w * (1 - max(0, j())), h * max(0, j())),
           (w * (1 - max(0, j())), h * (1 - max(0, j()))), (w * max(0, j()), h * (1 - max(0, j())))]
    # PIL wants the coefficients mapping OUTPUT -> INPUT
    A = []
    B = []
    for (xs, ys), (xd, yd) in zip(src, dst):
        A.append([xd, yd, 1, 0, 0, 0, -xs * xd, -xs * yd])
        A.append([0, 0, 0, xd, yd, 1, -ys * xd, -ys * yd])
        B += [xs, ys]
    coef = np.linalg.solve(np.array(A, dtype=float), np.array(B, dtype=float))
    return img.transform((w, h), Image.PERSPECTIVE, tuple(coef), Image.BICUBIC, fillcolor=255)


def degrade(img, blur, sigma, contrast, ramp):
    if blur > 0:
        img = img.filter(ImageFilter.GaussianBlur(blur))
    a = np.asarray(img, dtype=float)
    a = 128 + (a - 128) * contrast
    if ramp:
        h, w = a.shape
        gx = np.linspace(-ramp, ramp, w)[None, :]
        a = a + gx
    if sigma > 0:
        a = a + nprng.normal(0, sigma, a.shape)
    return Image.fromarray(np.clip(a, 0, 255).astype(np.uint8), "L")


def clutter(img):
    w, h = img.size
    big = Image.new("L", (int(w * rnd.uniform(1.6, 2.5)), int(h * rnd.uniform(1.6, 2.5))), 235)
    d = ImageDraw.Draw(big)
    for _ in range(40):
        x0, y0 = rnd.randrange(big.size[0]), rnd.randrange(big.size[1])
        d.line((x0, y0, x0 + rnd.randint(-80, 80), y0 + rnd.randint(-80, 80)), fill=rnd.randint(0, 120), width=rnd.randint(1, 4))
    ox = rnd.randint(10, big.size[0] - w - 10)
    oy = rnd.randint(10, big.size[1] - h - 10)
    big.paste(img, (ox, oy))
    return big


cases = []  # (category, payload, image)
tmp = tempfile.mkdtemp(prefix="qrdec-")
try:
    def add(cat, data, img):
        i = len(cases)
        path = os.path.join(tmp, "%05d.png" % i)
        img.save(path)
        cases.append((cat, data, path))

    for i in range(per):
        ecc = rnd.randrange(4)
        ver = rnd.choice([None, None, rnd.randint(1, 10), rnd.randint(10, 40)])
        cap = 40 if ver is None else min(40 + ver * 6, 400)
        data = payload(cap)
        try:
            m = make_matrix(data, ecc, ver)
        except Exception:
            continue
        n = m.shape[0]
        # --- clean
        scale = rnd.randint(2, 12) if n < 100 else rnd.randint(2, 6)
        add("clean", data, draw(m, scale, rnd.choice([4, 4, 2])))
        # --- rotated
        base = draw(m, max(4, scale), 4)
        add("rotated", data, rotate(base, rnd.uniform(0, 360)))
        # --- warped
        add("warped", data, warp(rotate(base, rnd.uniform(-20, 20)), 0.12))
        # --- blurred + noise + contrast + ramp
        sc = max(6, scale)
        add("blurred", data, degrade(rotate(draw(m, sc, 4), rnd.uniform(-15, 15)),
                                     rnd.uniform(0.4, 1.2) * sc / 6.0, rnd.uniform(5, 25),
                                     rnd.uniform(0.45, 1.0), rnd.uniform(0, 50)))
        # --- cluttered
        add("cluttered", data, clutter(draw(m, max(3, scale), 4)))

    # damaged: level H or Q, paint a block of modules (<= ~ 8% of the area)
    for i in range(per):
        ecc = rnd.choice([2, 3])
        data = payload(60)
        m = make_matrix(data, ecc, None).copy()
        n = m.shape[0]
        side = max(2, int(n * rnd.uniform(0.15, 0.27)))
        x0 = rnd.randrange(9, max(10, n - side - 9))
        y0 = rnd.randrange(9, max(10, n - side - 9))
        m[y0:y0 + side, x0:x0 + side] = nprng.random((side, side)) < 0.5
        add("damaged", data, draw(m, rnd.randint(4, 9), 4))

    # run both readers
    res = subprocess.run([cli] + [c[2] for c in cases], capture_output=True, check=True).stdout.decode().splitlines()
    assert len(res) == len(cases), (len(res), len(cases))

    cats = {}
    wrong = 0
    for (cat, data, path), line in zip(cases, res):
        f = line.split()
        ours = False
        if f[1] == "OK":
            got = bytes.fromhex(f[7]) if f[7] != "-" else b""
            if got == data:
                ours = True
            else:
                wrong += 1
                print("WRONG payload", path, cat, "ours", got[:30], "expected", data[:30])
        z = zxingcpp.read_barcodes(Image.open(path), formats=zxingcpp.BarcodeFormat.QRCode)
        theirs = len(z) >= 1 and any(o.bytes == data for o in z)
        c = cats.setdefault(cat, [0, 0, 0, 0])
        c[0] += 1
        c[1] += ours
        c[2] += theirs
        c[3] += (ours or theirs)
        if not ours:
            print("miss", cat, os.path.basename(path), "len", len(data), "zxing", "reads" if theirs else "misses", line[:60])
            if "--keep" in sys.argv:
                os.rename(path, "/tmp/qrmiss-%s-%s" % (cat, os.path.basename(path)))
    tot = [0, 0, 0]
    print("%-10s %6s %6s %6s" % ("category", "cases", "ours", "zxing"))
    for cat, c in cats.items():
        print("%-10s %6d %6d %6d" % (cat, c[0], c[1], c[2]))
        tot[0] += c[0]
        tot[1] += c[1]
        tot[2] += c[2]
    print("%-10s %6d %6d %6d   (wrong payloads: %d)" % ("total", tot[0], tot[1], tot[2], wrong))
    floors = {"clean": 1.0, "rotated": 0.97, "warped": 0.85, "blurred": 0.7, "cluttered": 0.9, "damaged": 0.9}
    fail = wrong > 0
    if "--floor" in sys.argv:
        for cat, c in cats.items():
            if c[0] and c[1] / c[0] < floors[cat]:
                print("FAIL %s: %.2f below the floor %.2f" % (cat, c[1] / c[0], floors[cat]))
                fail = True
    sys.exit(1 if fail else 0)
finally:
    import shutil
    shutil.rmtree(tmp, ignore_errors=True)
