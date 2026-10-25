#!/usr/bin/env python3
import io
import sys
import zipfile

from PIL import Image

sys.path.insert(0, __file__.rsplit("/", 1)[0] + "/../menu")
import layout_check as lc

WIDGETS = "assets/minecraft/textures/gui/widgets.png"
ICONS = "assets/minecraft/textures/gui/icons.png"


def load(jar, name):
    with zipfile.ZipFile(jar) as z:
        return Image.open(io.BytesIO(z.read(name))).convert("RGBA")


def match(img, tex, s, gx, gy, u, v, w, h, min_alpha=255):
    good = 0
    total = 0
    for dy in range(h * s):
        for dx in range(w * s):
            tr, tg, tb, ta = tex.getpixel((u + dx // s, v + dy // s))
            if ta < min_alpha:
                continue
            px = (gx * s + dx, gy * s + dy)
            if px[0] >= img.size[0] or px[1] >= img.size[1]:
                continue
            total += 1
            if img.getpixel(px)[:3] == (tr, tg, tb):
                good += 1
    return good, total


def read(path):
    w, h, data = lc.read_ppm(path)
    return Image.frombytes("RGB", (w, h), data)


def expect(name, good, total, errors, floor=0.97):
    ok = total > 0 and good / total >= floor
    print(f"  {'ok  ' if ok else 'FAIL'}  hud {name}: {good}/{total}")
    if not ok:
        errors.append(name)


def check_bars(img, jar, errors):
    widgets = load(jar, WIDGETS)
    icons = load(jar, ICONS)
    w, h = img.size
    s = lc.scale_for(w, h)
    gw = (w + s - 1) // s
    gh = (h + s - 1) // s
    hx = gw // 2 - 91
    g, t = match(img, widgets, s, hx + 1, gh - 21, 1, 1, 180, 20, 255)
    expect("hotbar", g, t, errors, 0.4)
    g, t = match(img, widgets, s, hx - 1, gh - 23, 0, 22, 24, 24, 255)
    expect("selected slot", g, t, errors, 0.9)
    top = gh - 39
    g, t = match(img, icons, s, hx, top, 52, 0, 9, 9, 255)
    expect("heart full", g, t, errors, 0.9)
    g, t = match(img, icons, s, gw // 2 + 91 - 9, top, 52, 27, 9, 9, 255)
    expect("food full", g, t, errors, 0.9)


def has_hotbar(img, jar):
    widgets = load(jar, WIDGETS)
    w, h = img.size
    s = lc.scale_for(w, h)
    gw = (w + s - 1) // s
    gh = (h + s - 1) // s
    g, t = match(img, widgets, s, gw // 2 - 91 + 1, gh - 21, 1, 1, 180, 20, 255)
    return t > 0 and g / t >= 0.4


def main():
    mode, ppm, jar = sys.argv[1], sys.argv[2], sys.argv[3]
    img = read(ppm)
    errors = []
    if mode == "bars":
        check_bars(img, jar, errors)
    elif mode == "hidden":
        ok = not has_hotbar(img, jar)
        print(f"  {'ok  ' if ok else 'FAIL'}  hud hidden with F1")
        if not ok:
            errors.append("hidden")
    elif mode == "debug":
        s = lc.scale_for(*img.size)
        px = img.getpixel((2 * s + 1, 2 * s + 1))
        bright = sum(1 for x in range(2 * s, 40 * s) for y in range(2 * s, 9 * s) if sum(img.getpixel((x, y))) > 600)
        ok = bright > 20
        print(f"  {'ok  ' if ok else 'FAIL'}  hud debug text pixels={bright}")
        if not ok:
            errors.append("debug")
    elif mode == "png":
        w, h = img.size
        ok = (w, h) == (int(sys.argv[4]), int(sys.argv[5]))
        print(f"  {'ok  ' if ok else 'FAIL'}  screenshot size {w}x{h}")
        if not ok:
            errors.append("png")
        check_bars(img, jar, errors)
    sys.exit(1 if errors else 0)


main()
