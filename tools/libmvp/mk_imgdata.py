#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/mk_imgdata.py <repo> -- (re)makes tests/data/webp and
# tests/data/gif (synthetic pictures, Pillow 12 / libwebp / libgif; no
# third-party content except FleiLauncher's own two logos, copied by hand)
# and prints the CRC-32 table that tests/2080_webp.fi and tests/2081_gif.fi
# compare against: the CRC-32 of Pillow's RGBA octets of the same file.
import os, random, sys, zlib
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from PIL import Image, ImageDraw
import pngmake
repo = sys.argv[1] if len(sys.argv) > 1 else "."
rng = random.Random(7)

def shapes(w, h, mode="RGB", n=40):
    im = Image.new(mode, (w, h), (0,) * len(mode))
    d = ImageDraw.Draw(im)
    for _ in range(n):
        x0, x1 = sorted([rng.randint(0, w - 1), rng.randint(0, w - 1)])
        y0, y1 = sorted([rng.randint(0, h - 1), rng.randint(0, h - 1)])
        col = tuple(rng.randint(0, 255) for _ in mode)
        d.ellipse([x0, y0, x1, y1], fill=col)
    for x in range(w):
        for y in range(0, h, 7):
            im.putpixel((x, y), tuple((x * (k + 2) + y * (k + 1)) % 256 for k in range(len(mode))))
    return im

wd = os.path.join(repo, "tests/data/webp")
gd = os.path.join(repo, "tests/data/gif")
os.makedirs(wd, exist_ok=True); os.makedirs(gd, exist_ok=True)
rgb = shapes(61, 47); rgba = shapes(61, 47, "RGBA")
rgba.putpixel((3, 3), (200, 10, 10, 0))
rgb.save(wd + "/ll_rgb.webp", lossless=True)
rgba.save(wd + "/ll_rgba.webp", lossless=True)
rgb.quantize(13).save(wd + "/ll_pal.webp", lossless=True)
rgb.save(wd + "/lossy_q75.webp", quality=75)
rgb.save(wd + "/lossy_q5.webp", quality=5)
rgba.save(wd + "/lossy_alpha.webp", quality=80)
rgba.save(wd + "/lossy_alpha_exact.webp", quality=80, exact=True)
shapes(1, 1).save(wd + "/lossy_1x1.webp", quality=80)
shapes(33, 2).save(wd + "/lossy_33x2.webp", quality=60, method=6)
fr = [shapes(48, 32, "RGBA", 10) for _ in range(6)]
fr[0].save(wd + "/anim_ll.webp", save_all=True, append_images=fr[1:], duration=[40, 80, 120, 40, 40, 200], loop=3, lossless=True)
fr[0].save(wd + "/anim_lossy.webp", save_all=True, append_images=fr[1:], duration=100, loop=0, quality=70)

q = shapes(61, 47)
for nc in (2, 4, 16, 256):
    q.quantize(nc).save(gd + "/pal%d.gif" % nc)
q.quantize(16).save(gd + "/inter.gif", interlace=True)
q.quantize(32).save(gd + "/transp.gif", transparency=5)
shapes(1, 1).quantize(2).save(gd + "/s1x1.gif")
noise = Image.frombytes("L", (120, 90), bytes(rng.randint(0, 255) for _ in range(120 * 90)))
noise.save(gd + "/noise.gif")  # poor compression: the code table fills up
pf = [shapes(50, 40).quantize(16) for _ in range(5)]
pf[0].save(gd + "/anim.gif", save_all=True, append_images=pf[1:], duration=[50, 100, 70, 20, 0], loop=0)
rf = []
for k in range(6):
    im = Image.new("RGBA", (48, 32), (0, 0, 0, 0)); d = ImageDraw.Draw(im)
    for _ in range(3):
        x0, x1 = sorted([rng.randint(0, 47), rng.randint(0, 47)]); y0, y1 = sorted([rng.randint(0, 31), rng.randint(0, 31)])
        d.rectangle([x0, y0, x1, y1], fill=(rng.randint(0, 255), rng.randint(0, 255), rng.randint(0, 255), 255))
    rf.append(im)
rf[0].save(gd + "/disp.gif", save_all=True, append_images=rf[1:], duration=80, loop=2, disposal=[1, 2, 3, 1, 2, 3])

# tests/data/img: the SAME opaque picture as PNG (colour type 2), lossless
# WebP and GIF, for tests/2082_image_from_bytes.fi; an RGBA PNG; a palette
# PNG (which lib/paint/png.fi refuses)
idir = os.path.join(repo, "tests/data/img")
os.makedirs(idir, exist_ok=True)
same = Image.open(gd + "/pal16.gif").convert("RGB")
same.save(idir + "/same.png"); same.save(idir + "/same.webp", lossless=True)
Image.open(gd + "/pal16.gif").save(idir + "/same.gif")
shapes(24, 16, "RGBA").save(idir + "/rgba.png")
shapes(24, 16).quantize(16).save(idir + "/palette.png")

# tests/data/png: every colour type / depth / interlace the PNG decoder takes,
# written by pngmake.py; the expectation is computed from the samples, not by
# a decoder. The CRC-32 table for tests/2083_png.fi is printed below.
pdir = os.path.join(repo, "tests/data/png")
os.makedirs(pdir, exist_ok=True)
prng = random.Random(21)
pal16 = [tuple(prng.randrange(256) for _ in range(3)) for _ in range(16)]
png_cases = [
    ("pal8_trns", 3, 8, 20, 13, False, pal16 * 4, bytes([0, 40, 128, 255, 7, 99])),
    ("pal4_i", 3, 4, 19, 11, True, pal16, None),
    ("pal2", 3, 2, 17, 5, False, pal16[:4], bytes([200, 100])),
    ("pal1", 3, 1, 9, 9, False, pal16[:2], None),
    ("grey2", 0, 2, 13, 7, False, None, None),
    ("grey4_i", 0, 4, 21, 9, True, None, None),
    ("grey8_key", 0, 8, 16, 8, False, None, "key"),
    ("grey16", 0, 16, 11, 6, False, None, None),
    ("rgb8_key", 2, 8, 14, 9, False, None, "key"),
    ("rgb16", 2, 16, 10, 7, False, None, None),
    ("la8", 4, 8, 12, 12, False, None, None),
    ("la16_i", 4, 16, 9, 13, True, None, None),
    ("rgba8_i", 6, 8, 23, 17, True, None, None),
    ("rgba16", 6, 16, 8, 8, False, None, None),
]
print("-- png")
for (name, ct, dp, w, h, il, plte, trns) in png_cases:
    data, samples = pngmake.make(prng, ct, dp, w, h, il, plte, None if trns == "key" else trns)
    if trns == "key":
        nch = pngmake.CH[ct]
        k = samples[3][2 * nch:2 * nch + nch]
        trns = b"".join(__import__("struct").pack(">H", v) for v in k)
        data, samples = pngmake.make(prng, ct, dp, w, h, il, plte, trns, samples)
    open(os.path.join(pdir, name + ".png"), "wb").write(data)
    print("%-12s %2dx%-2d ct %d depth %2d %s crc %d" % (name + ".png", w, h, ct, dp, "interlaced" if il else "          ",
          zlib.crc32(pngmake.expect(ct, dp, w, h, samples, plte, trns))))

def norm(b):
    b = bytearray(b)
    for i in range(0, len(b), 4):
        if b[i + 3] == 0:
            b[i] = b[i + 1] = b[i + 2] = 0
    return bytes(b)

for d, ext in ((wd, "webp"), (gd, "gif"), (idir, "png"), (idir, "webp"), (idir, "gif")):
    print("--", ext)
    for f in sorted(os.listdir(d)):
        if not f.endswith("." + ext):
            continue
        im = Image.open(os.path.join(d, f))
        n = getattr(im, "n_frames", 1)
        crcs = []
        for i in range(n):
            im.seek(i)
            b = im.convert("RGBA").tobytes()
            crcs.append(zlib.crc32(norm(b) if (ext == "gif" and i > 0) else b))
        print("%-22s %3dx%-3d frames %d first %d%s" % (f, im.size[0], im.size[1], n, crcs[0],
              ("  all(xor) %d" % (__import__("functools").reduce(lambda a, b: a ^ b, crcs))) if n > 1 else ""))
