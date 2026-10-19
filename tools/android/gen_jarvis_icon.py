#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Generate the JARVIS Android launcher icon (adaptive + legacy).

    python3 tools/android/gen_jarvis_icon.py [--out examples/jarvis_android/res] [--preview DIR]

Motif: broken accent-blue ring + glowing orb core on a dark navy background (palette of
public/style.css / examples/jarvis_android/jtokens.fi: bg #0c0e1f, accent #2f86ff, accent2 #36c5ff).

Output below --out (feed it to tools/android/build.sh --icon-res):
  mipmap-{m,h,xh,xxh,xxxh}dpi/ic_launcher.png          legacy square icon   48 72 96 144 192 px
  mipmap-*/ic_launcher_round.png                       legacy round icon
  mipmap-*/ic_launcher_foreground.png                  adaptive layer, 108 dp (108 162 216 324 432 px)
  mipmap-*/ic_launcher_monochrome.png                  themed icon silhouette (Android 13+), same size
  mipmap-anydpi-v26/ic_launcher.xml, ic_launcher_round.xml
  drawable/ic_launcher_background.xml                  radial dark gradient
  values/ic_launcher_background.xml                    flat fallback colour
All geometry is in dp on the 108 dp adaptive canvas; the safe zone is the centred 66 dp circle.
Deterministic: same input -> byte-identical PNGs (Pillow only, no randomness).
"""
import argparse
import math
import os
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter

BG = (0x0C, 0x0E, 0x1F)
BG_CENTER = (0x1B, 0x22, 0x4A)
ACCENT = (0x2F, 0x80, 0xFF)
ACCENT2 = (0x36, 0xC5, 0xFF)

DENSITIES = [("mdpi", 1.0), ("hdpi", 1.5), ("xhdpi", 2.0), ("xxhdpi", 3.0), ("xxxhdpi", 4.0)]
CANVAS_DP = 108.0
LEGACY_DP = 48.0
VISIBLE_DP = 72.0  # a legacy icon shows the centre 72 dp of the adaptive canvas

# geometry in dp (inside the 66 dp safe circle: outer radius + glow <= 33)
RING_OUTER = 27.0
RING_INNER = 20.5
GAP_DEG = 16.0
GAPS = (35.0, 215.0)  # gap centres, degrees (0 = right, counter-clockwise on screen: y down => clockwise)
CORE_R = 11.5
SS = 4  # supersampling


def lerp(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def diag_gradient(n, c0, c1):
    """Linear gradient from top-left (c0) to bottom-right (c1)."""
    g = Image.linear_gradient("L").rotate(90).resize((n, n))  # horizontal
    g2 = Image.linear_gradient("L").resize((n, n))  # vertical
    t = ImageChops.add(g, g2, scale=2)
    a = Image.new("RGB", (n, n), c0)
    b = Image.new("RGB", (n, n), c1)
    return Image.composite(b, a, t)


def radial(n, cx, cy, r, inner, outer):
    """Radial colour field: `inner` at the centre -> `outer` at radius r and beyond."""
    t = Image.radial_gradient("L").resize((int(2 * r), int(2 * r)))  # 0 centre .. 255 edge
    full = Image.new("L", (n, n), 255)
    full.paste(t, (int(cx - r), int(cy - r)))
    return Image.composite(Image.new("RGB", (n, n), outer), Image.new("RGB", (n, n), inner), full)


def ring_mask(n, u):
    """Broken ring as an L mask (255 = ring)."""
    m = Image.new("L", (n, n), 0)
    d = ImageDraw.Draw(m)
    c = n / 2
    d.ellipse([c - RING_OUTER * u, c - RING_OUTER * u, c + RING_OUTER * u, c + RING_OUTER * u], fill=255)
    d.ellipse([c - RING_INNER * u, c - RING_INNER * u, c + RING_INNER * u, c + RING_INNER * u], fill=0)
    cut = Image.new("L", (n, n), 0)
    cd = ImageDraw.Draw(cut)
    for gc in GAPS:
        # a wedge from the centre, wider than the ring, cuts the gap
        a0, a1 = gc - GAP_DEG / 2, gc + GAP_DEG / 2
        pts = [(c, c)]
        for k in range(0, 21):
            a = math.radians(a0 + (a1 - a0) * k / 20)
            pts.append((c + math.cos(a) * 40 * u, c + math.sin(a) * 40 * u))
        cd.polygon(pts, fill=255)
    return ImageChops.subtract(m, cut)


def core_mask(n, u):
    m = Image.new("L", (n, n), 0)
    c = n / 2
    ImageDraw.Draw(m).ellipse([c - CORE_R * u, c - CORE_R * u, c + CORE_R * u, c + CORE_R * u], fill=255)
    return m


def foreground(n):
    """Adaptive foreground (RGBA, transparent around the motif) at n x n px for the 108 dp canvas."""
    big = n * SS
    u = big / CANVAS_DP
    ring = ring_mask(big, u)
    core = core_mask(big, u)
    grad = diag_gradient(big, ACCENT2, ACCENT)
    # orb: bright cyan highlight top-left -> accent blue edge
    c = big / 2
    orb = radial(big, c - 3.5 * u, c - 3.5 * u, CORE_R * 1.9 * u, (0xD8, 0xF6, 0xFF), ACCENT)
    orb = Image.composite(orb, Image.new("RGB", (big, big), ACCENT), Image.new("L", (big, big), 255))
    out = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    # soft glow behind ring and orb
    glow_src = ImageChops.lighter(ring, core)
    glow = glow_src.filter(ImageFilter.GaussianBlur(2.6 * u)).point(lambda v: int(v * 0.55))
    out.paste(Image.new("RGBA", (big, big), ACCENT2 + (255,)), (0, 0), glow)
    out.paste(grad.convert("RGBA"), (0, 0), ring)
    out.paste(orb.convert("RGBA"), (0, 0), core)
    return out.resize((n, n), Image.LANCZOS)


def monochrome(n):
    """Themed-icon silhouette: opaque black ring + core; the system tints by alpha only."""
    big = n * SS
    u = big / CANVAS_DP
    m = ImageChops.lighter(ring_mask(big, u), core_mask(big, u))
    out = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    out.paste(Image.new("RGBA", (big, big), (0, 0, 0, 255)), (0, 0), m)
    return out.resize((n, n), Image.LANCZOS)


def background(n):
    """Dark navy with a soft lighter centre (RGB)."""
    return radial(n, n / 2, n / 2, n * 0.62, BG_CENTER, BG)


def composite_legacy(px, shape):
    """Legacy launcher icon px x px: adaptive composite scaled so 72 dp fill the icon, then masked."""
    scale = px / VISIBLE_DP  # px per dp
    n = int(round(CANVAS_DP * scale))
    comp = background(n).convert("RGBA")
    comp.alpha_composite(foreground(n))
    off = (n - px) // 2
    img = comp.crop((off, off, off + px, off + px))
    big = px * SS
    mask = Image.new("L", (big, big), 0)
    d = ImageDraw.Draw(mask)
    if shape == "round":
        d.ellipse([0, 0, big - 1, big - 1], fill=255)
    else:
        r = int(big * 0.2)
        d.rounded_rectangle([0, 0, big - 1, big - 1], radius=r, fill=255)
    mask = mask.resize((px, px), Image.LANCZOS)
    out = Image.new("RGBA", (px, px), (0, 0, 0, 0))
    out.paste(img, (0, 0), mask)
    return out


ADAPTIVE_XML = """<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background" />
    <foreground android:drawable="@mipmap/ic_launcher_foreground" />
    <monochrome android:drawable="@mipmap/ic_launcher_monochrome" />
</adaptive-icon>
"""

BG_DRAWABLE = """<?xml version="1.0" encoding="utf-8"?>
<shape xmlns:android="http://schemas.android.com/apk/res/android" android:shape="rectangle">
    <gradient android:type="radial" android:gradientRadius="62%%p"
        android:centerX="0.5" android:centerY="0.5"
        android:startColor="#%02X%02X%02X" android:endColor="#%02X%02X%02X" />
</shape>
""" % (BG_CENTER + BG)

BG_VALUES = """<?xml version="1.0" encoding="utf-8"?>
<resources>
    <color name="ic_launcher_background_color">#%02X%02X%02X</color>
</resources>
""" % BG


def write(path, data, mode="w"):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, mode) as f:
        f.write(data)


def save_png(img, path):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    img.save(path, "PNG", optimize=True)


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.normpath(os.path.join(here, "..", ".."))
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(root, "examples", "jarvis_android", "res"))
    ap.add_argument("--preview", default="", help="also write preview PNGs (192 px legacy, 432 px layers, 512 px store) here")
    a = ap.parse_args()
    for name, d in DENSITIES:
        base = os.path.join(a.out, "mipmap-" + name)
        save_png(composite_legacy(int(round(LEGACY_DP * d)), "square"), os.path.join(base, "ic_launcher.png"))
        save_png(composite_legacy(int(round(LEGACY_DP * d)), "round"), os.path.join(base, "ic_launcher_round.png"))
        n = int(round(CANVAS_DP * d))
        save_png(foreground(n), os.path.join(base, "ic_launcher_foreground.png"))
        save_png(monochrome(n), os.path.join(base, "ic_launcher_monochrome.png"))
    write(os.path.join(a.out, "mipmap-anydpi-v26", "ic_launcher.xml"), ADAPTIVE_XML)
    write(os.path.join(a.out, "mipmap-anydpi-v26", "ic_launcher_round.xml"), ADAPTIVE_XML)
    write(os.path.join(a.out, "drawable", "ic_launcher_background.xml"), BG_DRAWABLE)
    write(os.path.join(a.out, "values", "ic_launcher_background.xml"), BG_VALUES)
    if a.preview:
        os.makedirs(a.preview, exist_ok=True)
        save_png(composite_legacy(192, "square"), os.path.join(a.preview, "icon_192.png"))
        save_png(composite_legacy(192, "round"), os.path.join(a.preview, "icon_round_192.png"))
        save_png(composite_legacy(512, "square"), os.path.join(a.preview, "playstore_512.png"))
        fg = foreground(432)
        full = background(432).convert("RGBA")
        full.alpha_composite(fg)
        save_png(full, os.path.join(a.preview, "adaptive_full_432.png"))
        save_png(fg, os.path.join(a.preview, "foreground_432.png"))
        mono = monochrome(432)
        tint = Image.new("RGBA", (432, 432), (30, 30, 36, 255))
        t2 = Image.new("RGBA", (432, 432), (200, 215, 255, 255))
        tint.paste(t2, (0, 0), mono.split()[3])
        save_png(tint, os.path.join(a.preview, "monochrome_432.png"))
    print("icon resources written to", a.out)


if __name__ == "__main__":
    sys.exit(main())
