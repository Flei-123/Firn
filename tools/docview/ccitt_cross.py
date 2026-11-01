#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import io
import os
import random
import subprocess
import sys
import tempfile

from PIL import Image


def make_noise(rng, w, h, density):
    img = Image.new("1", (w, h), 1)
    px = img.load()
    for y in range(h):
        for x in range(w):
            if rng.random() < density:
                px[x, y] = 0
    return img


def make_runs(rng, w, h, maxrun):
    img = Image.new("1", (w, h), 1)
    px = img.load()
    for y in range(h):
        x = 0
        color = rng.randint(0, 1)
        while x < w:
            r = rng.randint(0, maxrun)
            for i in range(x, min(w, x + r)):
                px[i, y] = color
            x += max(r, 1)
            color ^= 1
    return img


def make_all_runs(w):
    h = 2 * w // 64 + 2
    img = Image.new("1", (w, h), 1)
    px = img.load()
    row = 0
    for length in range(0, w):
        for color in (0, 1):
            y = (2 * length + color) % h
            for x in range(length):
                px[x, y] = color if length < w else 1
        row += 1
    return img


def make_shapes(rng, w, h):
    img = Image.new("1", (w, h), 1)
    from PIL import ImageDraw
    dr = ImageDraw.Draw(img)
    for _ in range(max(3, w * h // 4000)):
        x0 = rng.randint(0, w - 1)
        y0 = rng.randint(0, h - 1)
        x1 = min(w - 1, x0 + rng.randint(0, w // 3 + 1))
        y1 = min(h - 1, y0 + rng.randint(0, h // 3 + 1))
        kind = rng.randint(0, 2)
        if kind == 0:
            dr.rectangle([x0, y0, x1, y1], fill=0)
        elif kind == 1:
            dr.line([x0, y0, x1, y1], fill=0, width=rng.randint(1, 3))
        else:
            dr.ellipse([x0, y0, x1, y1], outline=0)
    return img


def encode(img, mode):
    buf = io.BytesIO()
    if mode == "g4":
        img.save(buf, "TIFF", compression="group4")
    elif mode == "g3_1d":
        img.save(buf, "TIFF", compression="group3")
    elif mode == "g3_2d":
        img.save(buf, "TIFF", compression="group3", tiffinfo={292: 1})
    elif mode == "g3_1d_align":
        img.save(buf, "TIFF", compression="group3", tiffinfo={292: 4})
    elif mode == "g3_2d_align":
        img.save(buf, "TIFF", compression="group3", tiffinfo={292: 5})
    elif mode == "mh":
        img.save(buf, "TIFF", compression="tiff_ccitt")
    data = buf.getvalue()
    t = Image.open(io.BytesIO(data))
    offs = t.tag_v2[273]
    cnts = t.tag_v2[279]
    if isinstance(offs, int):
        offs, cnts = (offs,), (cnts,)
    rps = t.tag_v2.get(278, t.size[1])
    strips = [data[o:o + c] for o, c in zip(offs, cnts)]
    return strips, rps, t.convert("L")


PARAMS = {
    "g4": (-1, ""),
    "g3_1d": (0, ""),
    "g3_2d": (4, ""),
    "g3_1d_align": (0, "ae"),
    "g3_2d_align": (4, "ae"),
    "mh": (0, "a"),
}


def run(binary, raw, k, w, h, flags, work):
    path = os.path.join(work, "in.bin")
    with open(path, "wb") as f:
        f.write(raw)
    r = subprocess.run([binary, path, str(k), str(w), str(h), flags + "b"], capture_output=True, timeout=120)
    return r.stdout, r.returncode


def expected(ref, w, h):
    rb = (w + 7) // 8
    out = bytearray()
    px = ref.load()
    for y in range(h):
        row = bytearray(rb)
        for x in range(w):
            if px[x, y] >= 128:
                row[x >> 3] |= 0x80 >> (x & 7)
        out += row
    return bytes(out)


def main():
    binary = sys.argv[1]
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    rng = random.Random(seed)
    cases = []
    for w in (1, 7, 8, 9, 31, 64, 100, 400, 1728):
        for h in (1, 5, 40):
            cases.append(("noise", make_noise(rng, w, h, rng.choice((0.02, 0.2, 0.5, 0.9)))))
            cases.append(("runs", make_runs(rng, w, h, rng.choice((3, 20, 200)))))
            cases.append(("shapes", make_shapes(rng, w, h)))
    cases.append(("allruns", make_all_runs(2700)))
    cases.append(("allruns", make_all_runs(2600)))
    cases.append(("big", make_shapes(rng, 2480, 400)))
    total = 0
    bad = 0
    with tempfile.TemporaryDirectory() as work:
        for name, img in cases:
            w, h = img.size
            for mode, (k, flags) in PARAMS.items():
                try:
                    strips, rps, ref = encode(img, mode)
                except Exception as e:
                    continue
                want = expected(ref, w, h)
                got = b""
                code = 0
                left = h
                for strip in strips:
                    rows = min(rps, left)
                    part, code = run(binary, strip, k, w, rows, flags, work)
                    got += part
                    left -= rows
                total += 1
                if got != want:
                    bad += 1
                    if bad <= 15:
                        print("MISMATCH", name, mode, w, h, "code", code, "len", len(got), len(want))
    print("checks", total, "mismatches", bad)
    sys.exit(1 if bad else 0)


main()
