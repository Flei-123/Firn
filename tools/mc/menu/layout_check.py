#!/usr/bin/env python3
import io
import sys
import zipfile

from PIL import Image

WIDGETS = "assets/minecraft/textures/gui/widgets.png"
BTN_H = 20
NORMAL_V = 66
LANG_V = 106


def read_ppm(path):
    data = open(path, "rb").read()
    parts = data.split(b"\n", 3)
    w, h = (int(v) for v in parts[1].split())
    return w, h, parts[3]


def scale_for(w, h):
    s = min(w // 320, h // 240)
    return max(1, min(4, s))


def buttons(uw, uh, shift):
    cx = uw // 2 - 100
    j = uh // 4 + 48 + shift
    return [
        ("singleplayer", cx, j, 200),
        ("multiplayer", cx, j + 24, 200),
        ("realms", cx, j + 48, 200),
        ("options", cx, j + 84, 98),
        ("quit", uw // 2 + 2, j + 84, 98),
    ]


def sample(tex, x, y):
    return tex.getpixel((x, y))


def check_button(img, tex, s, x, y, w, errors, name):
    half = w // 2
    bad = 0
    checked = 0
    for dy in range(BTN_H * s):
        gy = dy // s
        for dx in range(w * s):
            gx = dx // s
            inner = 4 <= gx < w - 4 and 3 <= gy < BTN_H - 3
            if inner:
                continue
            if dx < half * s:
                tx = (2 * dx + 1) * half // (2 * half * s)
            else:
                ddx = dx - half * s
                tx = 200 - half + (2 * ddx + 1) * half // (2 * half * s)
            ty = NORMAL_V + (2 * dy + 1) * BTN_H // (2 * BTN_H * s)
            r, g, b, a = sample(tex, tx, ty)
            if a < 255:
                continue
            px = img.getpixel((x * s + dx, y * s + dy))
            checked += 1
            if max(abs(px[0] - r), abs(px[1] - g), abs(px[2] - b)) > 1:
                bad += 1
    if checked == 0 or bad > 0:
        errors.append("%s: %d of %d border pixels differ" % (name, bad, checked))
    label = 0
    for dy in range(4 * s, 16 * s):
        for dx in range(6 * s, (w - 6) * s):
            px = img.getpixel((x * s + dx, y * s + dy))
            if min(px[:3]) > 200:
                label += 1
    if label < 10:
        errors.append("%s: label text missing (%d bright pixels)" % (name, label))


def check_icon(img, tex, s, x, y, errors):
    bad = 0
    checked = 0
    for dy in range(20 * s):
        for dx in range(20 * s):
            tx = (2 * dx + 1) * 20 // (2 * 20 * s)
            ty = LANG_V + (2 * dy + 1) * 20 // (2 * 20 * s)
            r, g, b, a = sample(tex, tx, ty)
            if a < 255:
                continue
            px = img.getpixel((x * s + dx, y * s + dy))
            checked += 1
            if max(abs(px[0] - r), abs(px[1] - g), abs(px[2] - b)) > 1:
                bad += 1
    if checked == 0 or bad > 0:
        errors.append("language icon: %d of %d pixels differ" % (bad, checked))


def main():
    ppm, jar = sys.argv[1], sys.argv[2]
    shift = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    w, h, raw = read_ppm(ppm)
    img = Image.frombytes("RGB", (w, h), raw)
    with zipfile.ZipFile(jar) as z:
        tex = Image.open(io.BytesIO(z.read(WIDGETS))).convert("RGBA")
    s = scale_for(w, h)
    uw = (w + s - 1) // s
    uh = (h + s - 1) // s
    errors = []
    for name, x, y, bw in buttons(uw, uh, shift):
        check_button(img, tex, s, x, y, bw, errors, name)
    j = uh // 4 + 48 + shift
    check_icon(img, tex, s, uw // 2 - 124, j + 84, errors)
    if errors:
        for e in errors:
            print("  FAIL  %dx%d: %s" % (w, h, e))
        sys.exit(1)
    print("  ok    %dx%d scale %d: 5 buttons + language icon match" % (w, h, s))


if __name__ == "__main__":
    main()
