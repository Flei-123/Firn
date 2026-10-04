#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_png.py <png_probe> <workdir> [more.png | dir ...]
# -- lib/paint/png.fi (PNG in) against two references: Pillow (libpng) for
# every file, and for the files written here an expectation computed from
# the raw samples by the rules of the module header (16 bits keep the high
# octet; 1/2/4-bit grey scaled to 0..255; tRNS colour key and palette
# alpha; index beyond the palette black). Written here, with a PNG writer
# of our own: every colour type with every legal bit depth, Adam7 and
# non-interlaced, all five row filters, tRNS in all its forms, odd sizes
# (1 x 1 .. 37 x 29). Files and directories given as arguments (the Modrinth
# icons, mostly palette PNGs) are compared with Pillow.
import glob, os, random, struct, subprocess, sys, zlib
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:
    from PIL import Image
except ImportError:
    print("  skip: Pillow not installed"); sys.exit(0)
probe, work, extra = sys.argv[1], sys.argv[2], sys.argv[3:]
rng = random.Random(11)

from pngmake import *

cases = []
legal = {0: (1, 2, 4, 8, 16), 2: (8, 16), 3: (1, 2, 4, 8), 4: (8, 16), 6: (8, 16)}
n = 0
for ctype, depths in legal.items():
    for depth in depths:
        for interlace in (False, True):
            for (w, h) in ((1, 1), (2, 3), (7, 5), (8, 8), (13, 9), (37, 29)):
                plte = trns = None
                if ctype == 3:
                    plte = [tuple(rng.randrange(256) for _ in range(3)) for _ in range(rng.randint(1, 1 << depth))]
                    if rng.random() < 0.6: trns = bytes(rng.randrange(256) for _ in range(rng.randint(1, len(plte))))
                data, samples = make(rng, ctype, depth, w, h, interlace, plte, trns)
                if ctype in (0, 2) and rng.random() < 0.6:
                    # a colour key that really occurs (and one that does not)
                    nch = CH[ctype]; y, x = rng.randrange(h), rng.randrange(w)
                    k = samples[y][x * nch:x * nch + nch] if rng.random() < 0.7 else [rng.randrange(1 << depth) for _ in range(nch)]
                    trns = b"".join(struct.pack(">H", v) for v in k)
                    data, samples = make(rng, ctype, depth, w, h, interlace, plte, trns, samples)
                p = os.path.join(work, "c%d_d%d_%s_%dx%d.png" % (ctype, depth, "i" if interlace else "n", w, h))
                open(p, "wb").write(data)
                cases.append((p, expect(ctype, depth, w, h, samples, plte, trns), (w, h), ctype, depth if not (ctype in (0, 2) and trns) else -depth))
# a palette index beyond the palette
data, samples = make(rng, 3, 8, 9, 4, False, [(1, 2, 3), (200, 100, 50)], None, [[rng.randrange(6) for _ in range(9)] for _ in range(4)])
p = os.path.join(work, "oob.png"); open(p, "wb").write(data)
cases.append((p, expect(3, 8, 9, 4, samples, [(1, 2, 3), (200, 100, 50)], None), (9, 4), 3, 8))
# a big one
data, samples = make(rng, 6, 8, 300, 200, True)
p = os.path.join(work, "big.png"); open(p, "wb").write(data)
cases.append((p, expect(6, 8, 300, 200, samples, None, None), (300, 200), 6, 8))

out = os.path.join(work, "out.raw")
def run(p):
    r = subprocess.run([probe, p, out], capture_output=True, text=True)
    if r.stdout.strip() != "OK": return None, r.stdout.strip()
    raw = open(out, "rb").read(); hdr, px = raw.split(b"\n", 1)
    return (tuple(map(int, hdr.split())), px), None

bad = 0; pixels = 0
for (p, exp, size, ctype, depth) in cases:
    res, err = run(p)
    name = os.path.basename(p)
    if err: bad += 1; print("  FAIL %s: %s" % (name, err)); continue
    (sz, px) = res
    if sz != size or px != exp:
        bad += 1; print("  DIFF %s: own expectation (%d octets differ)" % (name, sum(1 for a, b in zip(px, exp) if a != b))); continue
    # and Pillow (libpng), where its conversion rules are the same: not for
    # 16-bit grey (I;16), and not for a tRNS colour key at a depth other than
    # 8 (Pillow compares the raw key with the already scaled 8-bit samples,
    # libpng and this decoder compare the original samples)
    if not (ctype == 0 and depth == 16) and not (depth < 0 and depth != -8):
        ref = Image.open(p).convert("RGBA")
        if ref.tobytes() != px:
            bad += 1; print("  DIFF %s: Pillow (%d octets differ)" % (name, sum(1 for a, b in zip(px, ref.tobytes()) if a != b)))
    pixels += size[0] * size[1]

files = []
for e in extra:
    if os.path.isdir(e):
        for f in sorted(glob.glob(os.path.join(e, "*"))):
            try:
                if open(f, "rb").read(8) == b"\x89PNG\r\n\x1a\n": files.append(f)
            except OSError: pass
    elif os.path.exists(e): files.append(e)
for f in files:
    hd = open(f, "rb").read(26)
    if len(hd) == 26 and hd[24] == 16 and hd[25] == 0:
        continue           # 16-bit grey: Pillow's I;16 conversion is not the 8-bit one (see above)
    try:
        ref = Image.open(f); ref.load(); ref = ref.convert("RGBA")
    except Exception:
        continue
    res, err = run(f)
    if err: bad += 1; print("  FAIL %s: %s" % (os.path.basename(f), err)); continue
    (sz, px) = res
    if sz != ref.size or px != ref.tobytes():
        bad += 1; print("  DIFF %s: Pillow" % os.path.basename(f))
    pixels += sz[0] * sz[1]
print("png: %d files (%d pixels) decoded octet for octet like Pillow %s / the samples, %d differ"
      % (len(cases) + len(files), pixels, Image.__version__, bad))
sys.exit(1 if bad else 0)
