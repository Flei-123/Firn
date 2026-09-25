#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_jpeg.py <jpeg_probe> <workdir> -- lib/jpeg against
# Pillow (libjpeg-turbo): the same RGBA, octet for octet, for baseline and
# progressive files, every chroma subsampling Pillow writes, restart
# markers, EXIF orientations, grey, CMYK, odd sizes -- and for the photos
# given as further arguments.
import os, random, subprocess, sys
try:
    from PIL import Image, ImageDraw, ImageOps
except ImportError:
    print("  skip: Pillow not installed"); sys.exit(0)
probe, work, extra = sys.argv[1], sys.argv[2], sys.argv[3:]
rng = random.Random(1)
base = Image.new("RGB", (123, 77))
d = ImageDraw.Draw(base)
for _ in range(60):
    x0, x1 = sorted([rng.randint(0, 120), rng.randint(0, 120)])
    y0, y1 = sorted([rng.randint(0, 70), rng.randint(0, 70)])
    d.ellipse([x0, y0, x1, y1], fill=(rng.randint(0, 255), rng.randint(0, 255), rng.randint(0, 255)))
for x in range(123):
    for y in range(0, 77, 7):
        base.putpixel((x, y), (x * 2 % 256, y * 3 % 256, (x + y) % 256))
big = base.resize((1003, 611), Image.BICUBIC)
files = []
def save(name, img, **kw):
    p = os.path.join(work, name)
    img.save(p, **kw)
    files.append(p)
for q in (10, 50, 85, 100):
    for s in (0, 1, 2):
        save("q%d_s%d.jpg" % (q, s), base, quality=q, subsampling=s)
        save("q%d_s%d_prog.jpg" % (q, s), base, quality=q, subsampling=s, progressive=True)
save("big420.jpg", big, quality=90, subsampling=2)
save("big_prog.jpg", big, quality=75, subsampling=2, progressive=True)
save("rst.jpg", base, quality=80, restart_marker_blocks=3)
save("rst_rows_prog.jpg", big, quality=80, progressive=True, restart_marker_rows=1, subsampling=2)
save("gray.jpg", base.convert("L"))
save("gray_prog.jpg", big.convert("L"), progressive=True)
save("cmyk.jpg", base.convert("CMYK"))
for o in range(1, 9):
    ex = Image.Exif(); ex[0x0112] = o
    save("exif%d.jpg" % o, base, exif=ex.tobytes(), subsampling=2)
for w, h in ((1, 1), (2, 2), (3, 17), (17, 3), (9, 9), (16, 16), (33, 1)):
    save("size_%dx%d.jpg" % (w, h), base.resize((w, h)), subsampling=2)
files += [f for f in extra if os.path.exists(f)]
bad = 0
pixels = 0
for f in files:
    out = os.path.join(work, "out.raw")
    r = subprocess.run([probe, f, out], capture_output=True, text=True)
    ref = ImageOps.exif_transpose(Image.open(f)).convert("RGBA")
    if r.stdout.strip() != "OK":
        bad += 1; print("  FAIL %s: %s" % (f, r.stdout.strip())); continue
    raw = open(out, "rb").read()
    hdr, px = raw.split(b"\n", 1)
    w, h = map(int, hdr.split())
    if (w, h) != ref.size or px != ref.tobytes():
        bad += 1
        n = sum(1 for a, b in zip(px, ref.tobytes()) if a != b) if (w, h) == ref.size else -1
        print("  FAIL %s: %dx%d vs %dx%d, %d octets differ" % (f, w, h, ref.size[0], ref.size[1], n))
    pixels += w * h
print("jpeg: %d files (%d pixels) decoded octet for octet like Pillow %s, %d differ"
      % (len(files), pixels, Image.__version__, bad))
sys.exit(1 if bad else 0)
