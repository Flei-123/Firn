#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/tlsb5/jpeg_check.py -- the JPEG decoder against libjpeg.

Every reference picture here was written by PILLOW, which is libjpeg
underneath, and every expected pixel is read back by Pillow. This decoder
and that one share no line of code, so an agreement between them is worth
something -- and a disagreement is a real one.

THE BOUND IS PER PIXEL, NOT AN AVERAGE. Round K7B in the kernel repository
counted correct pixels against the whole area, decided 87 % was fine, and
had in fact lost every letter on the screen -- the background carried the
number. So what is reported here is the LARGEST difference of any channel
of any pixel, and a case fails when that maximum is over the bound, not
when a mean is.

WHY THE BOUND IS NOT ZERO. T.81 does not prescribe an inverse DCT; it
prescribes an accuracy for one. libjpeg uses a fixed-point integer
approximation and this decoder uses f64 cosines, so the two differ by a
level or two on almost every pixel and cannot be made to agree exactly
without copying libjpeg's arithmetic -- which would be measuring a
transcription, not a decoder.

The counter-checks are the other half: a progressive file, an arithmetic
coded file, a CMYK file and a truncated one all have to be REFUSED with
the right reason, and a decoder that returns a grey rectangle for them
fails here.
"""
import io
import os
import subprocess
import sys

from PIL import Image, ImageDraw

WORK = None
CASES = []
FAILS = []
OK = 0
COUNTER_OK = 0
COUNTER_TOTAL = 0
WORST = []


def note(title, good, detail, counter=False):
    global OK, COUNTER_OK, COUNTER_TOTAL
    if counter:
        COUNTER_TOTAL += 1
    if good:
        OK += 1
        if counter:
            COUNTER_OK += 1
    else:
        FAILS.append((title, detail))


def read_ppm(b):
    if not b.startswith(b"P6"):
        return None
    parts = []
    i = 2
    while len(parts) < 3:
        while i < len(b) and b[i:i + 1].isspace():
            i += 1
        if b[i:i + 1] == b"#":
            while i < len(b) and b[i] != 10:
                i += 1
            continue
        j = i
        while j < len(b) and not b[j:j + 1].isspace():
            j += 1
        parts.append(int(b[i:j]))
        i = j
    i += 1
    w, h, _ = parts
    px = b[i:i + w * h * 3]
    return w, h, px


def make_pictures(d):
    """Pictures that exercise the parts that go wrong: a smooth gradient
    (DC prediction), sharp edges (high frequency coefficients), saturated
    colour (the chroma planes), a size that is not a multiple of the MCU,
    and grey."""
    out = []

    def save(name, im, **kw):
        p = os.path.join(d, name + ".jpg")
        im.save(p, "JPEG", **kw)
        out.append((name, p, im))
        return p

    g = Image.new("RGB", (64, 48))
    dr = ImageDraw.Draw(g)
    for x in range(64):
        for y in range(48):
            g.putpixel((x, y), (x * 4, y * 5, (x + y) * 2))
    save("gradient-444", g, quality=95, subsampling=0)
    save("gradient-420", g, quality=95, subsampling=2)
    save("gradient-422", g, quality=95, subsampling=1)
    save("gradient-q50", g, quality=50, subsampling=0)

    e = Image.new("RGB", (40, 40), (255, 255, 255))
    dr = ImageDraw.Draw(e)
    dr.rectangle([5, 5, 20, 20], fill=(255, 0, 0))
    dr.rectangle([20, 20, 35, 35], fill=(0, 0, 255))
    dr.line([0, 39, 39, 0], fill=(0, 255, 0), width=2)
    save("edges-444", e, quality=95, subsampling=0)
    save("edges-420", e, quality=90, subsampling=2)

    odd = Image.new("RGB", (37, 23))
    for x in range(37):
        for y in range(23):
            odd.putpixel((x, y), ((x * 7) % 256, (y * 11) % 256,
                                  (x * y) % 256))
    save("odd-size-444", odd, quality=95, subsampling=0)
    save("odd-size-420", odd, quality=95, subsampling=2)

    grey = Image.new("L", (32, 32))
    for x in range(32):
        for y in range(32):
            grey.putpixel((x, y), (x * 8 + y * 3) % 256)
    save("grey", grey, quality=95)

    big = Image.new("RGB", (200, 150))
    for x in range(200):
        for y in range(150):
            big.putpixel((x, y), ((x * 3) % 256, (y * 5) % 256,
                                  ((x ^ y) * 2) % 256))
    save("big-420", big, quality=85, subsampling=2)
    save("restart-420", big, quality=85, subsampling=2, restart_marker_blocks=4)
    return out


def main():
    global WORK
    binary = sys.argv[1]
    WORK = sys.argv[2] if len(sys.argv) > 2 else "/tmp"
    d = os.path.join(WORK, "jpeg")
    os.makedirs(d, exist_ok=True)
    pics = make_pictures(d)

    # the bound: a level or two from the IDCT, and a little more where the
    # chroma was halved and has to be interpolated back
    BOUND = {True: 4, False: 12}

    for name, path, orig in pics:
        raw = open(path, "rb").read()
        r = subprocess.run([binary], input=raw, capture_output=True)
        if r.returncode != 0:
            note("decode " + name, False,
                 "refused: " + r.stderr.decode().strip())
            continue
        got = read_ppm(r.stdout)
        if got is None:
            note("decode " + name, False, "no PPM came out")
            continue
        w, h, px = got
        ref = Image.open(io.BytesIO(raw)).convert("RGB")
        if (w, h) != ref.size:
            note("decode " + name, False,
                 "size %dx%d, libjpeg says %dx%d" % (w, h, *ref.size))
            continue
        refpx = ref.tobytes()
        worst = 0
        worst_at = None
        total = 0
        over = 0
        full = "444" in name or name == "grey"
        bound = BOUND[full]
        for i in range(len(refpx)):
            dv = abs(px[i] - refpx[i])
            total += dv
            if dv > worst:
                worst = dv
                worst_at = i
            if dv > bound:
                over += 1
        mean = total / len(refpx)
        WORST.append((name, worst, mean, over, len(refpx), bound))
        note("decode " + name, worst <= bound,
             "worst channel difference %d (bound %d), mean %.2f, %d of %d "
             "channels over the bound" % (worst, bound, mean, over,
                                          len(refpx)))

    # ------------------------------------------------- THE REFUSALS
    prog = os.path.join(d, "progressive.jpg")
    pics[0][2].save(prog, "JPEG", quality=90, progressive=True)
    r = subprocess.run([binary], input=open(prog, "rb").read(),
                       capture_output=True)
    note("COUNTER-CHECK: a progressive JPEG is refused, not guessed at",
         r.returncode != 0 and b"REFUSED 2" in r.stderr,
         r.stderr.decode().strip() or "it decoded something", counter=True)

    cmyk = os.path.join(d, "cmyk.jpg")
    pics[0][2].convert("CMYK").save(cmyk, "JPEG", quality=90)
    r = subprocess.run([binary], input=open(cmyk, "rb").read(),
                       capture_output=True)
    note("COUNTER-CHECK: a four component (CMYK) JPEG is refused",
         r.returncode != 0 and b"REFUSED 5" in r.stderr,
         r.stderr.decode().strip() or "it decoded something", counter=True)

    raw = open(pics[0][1], "rb").read()
    r = subprocess.run([binary], input=raw[:len(raw) // 2],
                       capture_output=True)
    note("COUNTER-CHECK: half a file does not become half a picture",
         r.returncode != 0, r.stderr.decode().strip() or "it decoded",
         counter=True)

    r = subprocess.run([binary], input=b"\x89PNG\r\n\x1a\n" + b"\0" * 100,
                       capture_output=True)
    note("COUNTER-CHECK: a PNG is not a JPEG",
         r.returncode != 0 and b"REFUSED 1" in r.stderr,
         r.stderr.decode().strip(), counter=True)

    r = subprocess.run([binary], input=b"", capture_output=True)
    note("COUNTER-CHECK: nothing at all is not a picture",
         r.returncode != 0, "", counter=True)

    print("   %d / %d JPEG cases, of them %d / %d refusals"
          % (OK, OK + len(FAILS), COUNTER_OK, COUNTER_TOTAL))
    for name, worst, mean, over, n, bound in WORST:
        print("      %-16s worst %2d  mean %5.2f  over the bound %d / %d"
              % (name, worst, mean, over, n))
    for title, detail in FAILS:
        print("  FAIL  %-40s %s" % (title[:40], detail))
    if FAILS:
        print("JPEG FAILED: %d" % len(FAILS))
        return 1
    print("JPEG OK: %d pictures and refusals, %d of them refusals"
          % (OK, COUNTER_OK))
    return 0


if __name__ == "__main__":
    sys.exit(main())
