#!/usr/bin/env python3
"""Generates the FirnCam launcher icon resources into examples/firncam/res/.

Adaptive icon (API 26+): vector background colour, vector foreground, vector monochrome layer (themed icons,
API 33+). Fallback PNGs (mipmap-*dpi) are drawn with the same shapes for launchers that ignore the XML.
Usage: python3 -I gen_icon.py <out-res-dir>     (Pillow needed for the PNGs)
"""
import os
import sys
from PIL import Image, ImageDraw

BG = "#11151B"
RING = "#F4F6F8"
ACCENT = "#FF5A47"
CORE = "#DCE3EA"

# shapes in the 108 x 108 adaptive viewport, centred on (54, 54), everything inside the 66 dp safe circle
OUTER_R, OUTER_W = 24.0, 4.5
INNER_R, INNER_W = 14.0, 4.0
CORE_R = 6.0
SPARK = (45.5, 45.5, 2.6)  # highlight dot


def circle_path(cx, cy, r):
    return "M%g,%g a%g,%g 0 1,0 %g,0 a%g,%g 0 1,0 %g,0 z" % (cx - r, cy, r, r, 2 * r, r, r, -2 * r)


def vector(layers):
    out = ['<?xml version="1.0" encoding="utf-8"?>',
           '<vector xmlns:android="http://schemas.android.com/apk/res/android"',
           '    android:width="108dp" android:height="108dp" android:viewportWidth="108" android:viewportHeight="108">']
    for path, fill, stroke, width, alpha in layers:
        a = '    <path android:pathData="%s"' % path
        if fill:
            a += ' android:fillColor="%s"' % fill
        if stroke:
            a += ' android:strokeColor="%s" android:strokeWidth="%g"' % (stroke, width)
        if alpha < 1.0:
            a += ' android:fillAlpha="%g" android:strokeAlpha="%g"' % (alpha, alpha)
        out.append(a + "/>")
    out.append("</vector>")
    return "\n".join(out) + "\n"


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)


def draw_png(size, round_shape):
    ss = 4
    n = size * ss
    im = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    if round_shape:
        d.ellipse([0, 0, n - 1, n - 1], fill=BG)
    else:
        d.rounded_rectangle([0, 0, n - 1, n - 1], radius=int(n * 0.22), fill=BG)
    # the glyph spans ~ 72 of the 108 viewport units
    k = n / 72.0

    def px(v):
        return (v - 18.0) * k

    def ring(r, w, col):
        d.ellipse([px(54 - r), px(54 - r), px(54 + r), px(54 + r)], outline=col, width=max(1, int(w * k)))

    ring(OUTER_R, OUTER_W, RING)
    ring(INNER_R, INNER_W, ACCENT)
    d.ellipse([px(54 - CORE_R), px(54 - CORE_R), px(54 + CORE_R), px(54 + CORE_R)], fill=CORE)
    sx, sy, sr = SPARK
    d.ellipse([px(sx - sr), px(sy - sr), px(sx + sr), px(sy + sr)], fill=RING)
    return im.resize((size, size), Image.LANCZOS)


def main():
    out = sys.argv[1]
    shapes = [
        (circle_path(54, 54, OUTER_R), None, RING, OUTER_W, 1.0),
        (circle_path(54, 54, INNER_R), None, ACCENT, INNER_W, 1.0),
        (circle_path(54, 54, CORE_R), CORE, None, 0, 1.0),
        (circle_path(SPARK[0], SPARK[1], SPARK[2]), RING, None, 0, 1.0),
    ]
    mono = [
        (circle_path(54, 54, OUTER_R), None, "#FF000000", OUTER_W, 1.0),
        (circle_path(54, 54, INNER_R), None, "#FF000000", INNER_W, 1.0),
        (circle_path(54, 54, CORE_R), "#FF000000", None, 0, 1.0),
    ]
    write(os.path.join(out, "drawable/ic_launcher_foreground.xml"), vector(shapes))
    write(os.path.join(out, "drawable/ic_launcher_monochrome.xml"), vector(mono))
    write(os.path.join(out, "values/ic_launcher_background.xml"),
          '<?xml version="1.0" encoding="utf-8"?>\n<resources>\n    <color name="ic_launcher_background">%s</color>\n</resources>\n' % BG)
    adaptive = ('<?xml version="1.0" encoding="utf-8"?>\n'
                '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
                '    <background android:drawable="@color/ic_launcher_background"/>\n'
                '    <foreground android:drawable="@drawable/ic_launcher_foreground"/>\n'
                '    <monochrome android:drawable="@drawable/ic_launcher_monochrome"/>\n'
                '</adaptive-icon>\n')
    write(os.path.join(out, "mipmap-anydpi-v26/ic_launcher.xml"), adaptive)
    write(os.path.join(out, "mipmap-anydpi-v26/ic_launcher_round.xml"), adaptive)
    for dens, px_ in (("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)):
        for name, rnd in (("ic_launcher", False), ("ic_launcher_round", True)):
            p = os.path.join(out, "mipmap-%s/%s.png" % (dens, name))
            os.makedirs(os.path.dirname(p), exist_ok=True)
            draw_png(px_, rnd).save(p)
    draw_png(512, False).save(os.path.join(os.path.dirname(out.rstrip("/")), "res_src/playstore-512.png"))


main()
