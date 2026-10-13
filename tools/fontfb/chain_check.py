#!/usr/bin/env python3
"""tools/fontfb/chain_check.py -- font chain, CBDT/CBLC bitmap glyphs and clusters against
Pillow / FreeType, fontTools and (if importable) the `regex` module.

usage: chain_check.py <chainprobe binary>

  A  bitmap glyphs at the native strike size (109 px) vs Pillow/FreeType (embedded colour,
     layout_engine BASIC): max channel difference <= 2
  B  the same glyphs vs the PNG inside the font (fontTools), composited with the same formula: exact (<= 1)
  C  scaled drawing (shrink 20, 32, 64 px; enlarge 150 px) vs an independent numpy
     implementation (area average / bilinear, premultiplied): <= 1
  D  a text run "Ab<emoji>e-acute" in a chain [Liberation, Noto Color Emoji] at 32 px: the emoji part
     vs numpy (<= 2), the letters vs Pillow/FreeType (mean difference measured, bounded)
  E  runs (font choice per cluster) and total width vs fontTools cmap/hmtx + regex \\X clusters
"""
import os
import random
import subprocess
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from fontTools.ttLib import TTFont

probe = sys.argv[1]
NOTO = "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf"
DEJAVU = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
LIB = "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf"
WHITE = 0xFFFFFFFF
fails = 0
checks = 0


def check(ok, msg):
    global fails, checks
    checks += 1
    if not ok:
        fails += 1
        print("  FAIL", msg)


def sh(args):
    r = subprocess.run([probe] + [str(a) for a in args], capture_output=True, timeout=120)
    if r.returncode != 0:
        print("  FAIL probe", args[:3], "exit", r.returncode, r.stderr[:200])
        sys.exit(1)
    return r.stdout


def glyph_img(cp, px, w, h, x0, base, bg=WHITE):
    d = sh(["glyph", NOTO, cp, px, w, h, x0, base, bg])
    return np.frombuffer(d, dtype=np.uint8).reshape(h, w, 4).astype(int)


noto = TTFont(NOTO)
cmap = noto.getBestCmap()
strike0 = noto["CBDT"].strikeData[0]
ppem = noto["CBLC"].strikes[0].bitmapSizeTable.ppemY


def png_of(cp):
    g = strike0[cmap[cp]]
    import io
    return Image.open(io.BytesIO(g.imageData)).convert("RGBA"), g.metrics.BearingX, g.metrics.BearingY


def composite(P, A, bg_rgb):
    """P premultiplied (h,w,3) 0..255, A (h,w) 0..1, bg (3,) -> rounded rgb"""
    return np.floor(P + np.array(bg_rgb)[None, None, :] * (1.0 - A[:, :, None]) + 0.5)


def premul(img):
    a = np.asarray(img).astype(float)
    A = a[:, :, 3] / 255.0
    return a[:, :, :3] * A[:, :, None], A


def scaled_ref(cp, px, W, H, pen, base):
    img, bx, by = png_of(cp)
    Pm, A = premul(img)
    h, w = A.shape
    s = px / ppem
    left = pen + bx * s
    top = base - by * s
    Pm4 = np.concatenate([Pm, A[:, :, None]], axis=2)
    if s <= 1.0:
        def wts(n_src, n_dst, off):
            m = np.zeros((n_dst, n_src))
            for X in range(n_dst):
                u0 = (X - off) / s
                u1 = (X + 1 - off) / s
                for j in range(max(0, int(np.floor(u0))), min(n_src, int(np.ceil(u1)))):
                    ov = min(u1, j + 1) - max(u0, j)
                    if ov > 0:
                        m[X, j] = ov / (u1 - u0)
            return m
        wy = wts(h, H, top)
        wx = wts(w, W, left)
        out = np.einsum("hkc,xk->hxc", np.tensordot(wy, Pm4, axes=(1, 0)), wx)
    else:
        def wts(n_src, n_dst, off):
            m = np.zeros((n_dst, n_src))
            for X in range(n_dst):
                u = (X + 0.5 - off) / s - 0.5
                ix = int(np.floor(u))
                fx = u - ix
                for j, wgt in ((ix, 1 - fx), (ix + 1, fx)):
                    if 0 <= j < n_src and -1.0 < u < n_src:
                        m[X, j] += wgt
            return m
        wy = wts(h, H, top)
        wx = wts(w, W, left)
        out = np.einsum("hkc,xk->hxc", np.tensordot(wy, Pm4, axes=(1, 0)), wx)
    return composite(out[:, :, :3], out[:, :, 3], (255, 255, 255)).astype(int)


# candidate code points: bitmap glyphs of the first strike
rnd = random.Random(5)
all_cps = sorted(c for c in cmap if cmap[c] in strike0 and c > 0x2000)
sample = [0x1F600, 0x1F984, 0x1F1E6, 0x2764, 0x1F3F3] + rnd.sample(all_cps, 25)
sample = [c for c in sample if c in cmap and cmap[c] in strike0]

# ---- A: native size vs Pillow
fnt = ImageFont.truetype(NOTO, 109, layout_engine=ImageFont.Layout.BASIC)
worst = 0
for cp in sample[:20]:
    W, H = 180, 170
    ours = glyph_img(cp, 109, W, H, 20, 140)
    ref = Image.new("RGBA", (W, H), (255, 255, 255, 255))
    ImageDraw.Draw(ref).text((20, 140), chr(cp), font=fnt, embedded_color=True, anchor="ls")
    dmax = int(np.abs(ours - np.array(ref).astype(int)).max())
    worst = max(worst, dmax)
    check(dmax <= 2, "A native U+%04X vs Pillow: max diff %d" % (cp, dmax))
print("  A native size vs Pillow/FreeType: %d glyphs, worst channel diff %d (<= 2)" % (min(20, len(sample)), worst))

# ---- B: exact vs the PNG in the font
worst = 0
for cp in sample:
    W, H = 180, 170
    ours = glyph_img(cp, 109, W, H, 20, 140)
    img, bx, by = png_of(cp)
    Pm, A = premul(img)
    ref = np.full((H, W, 3), 255.0)
    h, w = A.shape
    # the PNG sits at (20 + bx, 140 - by)
    x0, y0 = 20 + bx, 140 - by
    reg = composite(Pm, A, (255, 255, 255))
    ref[y0:y0 + h, x0:x0 + w, :] = reg
    dmax = int(np.abs(ours[:, :, :3] - ref).max())
    worst = max(worst, dmax)
    check(dmax <= 1, "B U+%04X vs PNG: max diff %d" % (cp, dmax))
print("  B vs embedded PNG (alignment by bearings): %d glyphs, worst diff %d (<= 1)" % (len(sample), worst))

# ---- C: scaling
worst = 0
n = 0
for px in (20, 32, 64, 150):
    for cp in sample[:6]:
        W, H = int(px * 1.5) + 8, int(px * 1.5) + 8
        base = int(px * 1.2)
        ours = glyph_img(cp, px, W, H, 3, base)
        ref = scaled_ref(cp, px, W, H, 3.0, float(base))
        dmax = int(np.abs(ours[:, :, :3] - ref).max())
        worst = max(worst, dmax)
        n += 1
        check(dmax <= 1, "C px %d U+%04X vs numpy: max diff %d" % (px, cp, dmax))
print("  C scaled (shrink 20/32/64, enlarge 150): %d images, worst diff %d (<= 1)" % (n, worst))

# ---- D: text run
cp = 0x1F600
text = "Ab" + chr(cp) + "é"
px, W, H, base, x0 = 32, 150, 48, 36, 4
d = sh(["paint", px, W, H, x0, base, WHITE, LIB, "e:" + NOTO, "--", text])
ours = np.frombuffer(d, dtype=np.uint8).reshape(H, W, 4).astype(int)
lib = TTFont(LIB)
lcm = lib.getBestCmap()
check(cp not in lcm, "D precondition: Liberation lacks U+1F600")
upem = lib["head"].unitsPerEm
pen = float(x0)
for ch in "Ab":
    pen += lib["hmtx"][lcm[ord(ch)]][0] * (px / upem)
emoji_left = pen
ref_e = scaled_ref(cp, px, W, H, pen, float(base))
# emoji box (+1 px margin) only
s = px / ppem
img, bx, by = png_of(cp)
ex0 = int(np.floor(pen + bx * s)) - 1
ex1 = int(np.floor(pen + (bx + img.size[0]) * s)) + 1
dmax = int(np.abs(ours[:, ex0:ex1, :3] - ref_e[:, ex0:ex1, :]).max())
check(dmax <= 2, "D emoji in run vs numpy: max diff %d" % dmax)
print("  D emoji inside a text run vs numpy: max diff %d (<= 2)" % dmax)
# reference: FreeType at 8x, area-reduced to 1x (what an unhinted exact-coverage rasteriser gives;
# Pillow's own 1x output is grid-fitted, which the own rasteriser does not do)
K = 8
pf = ImageFont.truetype(LIB, px * K, layout_engine=ImageFont.Layout.BASIC)
big = Image.new("RGB", (W * K, H * K), (255, 255, 255))
ImageDraw.Draw(big).text((x0 * K, base * K), "Ab", font=pf, fill=(0, 0, 0), anchor="ls")
refim = big.reduce(K)
lx1 = ex0
dd = np.abs(ours[:, :lx1, :3] - np.array(refim).astype(int)[:, :lx1, :])
mean = float(dd.mean())
ink = float((np.array(refim)[:, :lx1, :] < 250).mean())
check(mean < 1.5 and ink > 0.01, "D letters vs FreeType 8x: mean diff %.3f" % mean)
print("  D letters vs FreeType (8x, area-reduced; own rasteriser): mean abs diff %.3f of 255 (< 1.5), max %d" % (mean, int(dd.max())))

# ---- E: runs + width vs fontTools / regex
try:
    import regex
except ImportError:
    regex = None
fonts = [TTFont(DEJAVU), TTFont(LIB), noto]
FS = [DEJAVU, "e:" + NOTO]   # chain used in the probe
chain = [TTFont(DEJAVU), noto]
cms = [f.getBestCmap() for f in chain]
kinds = [0, 3]


def extender(cp):
    return cp == 0x200D or 0xFE00 <= cp <= 0xFE0F or 0x1F3FB <= cp <= 0x1F3FF or 0xE0100 <= cp <= 0xE01EF or 0xE0020 <= cp <= 0xE007F or cp == 0x20E3


def wants(cl):
    return any(ord(c) in (0xFE0F, 0x200D, 0x20E3) or 0x1F3FB <= ord(c) <= 0x1F3FF or 0x1F1E6 <= ord(c) <= 0x1F1FF for c in cl)


def pick(cp, pref):
    if pref:
        for i, cm in enumerate(cms):
            if kinds[i] == 3 and cp in cm and cm[cp] != ".notdef" and chain[i].getGlyphID(cm[cp]) != 0:
                return i
    for i, cm in enumerate(cms):
        if cp in cm and chain[i].getGlyphID(cm[cp]) != 0:
            return i
    return None


def exp_runs(text):
    runs = []
    off = 0
    for cl in regex.findall(r"\X", text):
        k = len(cl.encode("utf-8"))
        pref = wants(cl)
        fi = pick(ord(cl[0]), pref)
        if fi is None:
            fi = 0
        if runs and runs[-1][2] == fi and not pref:
            runs[-1][1] += k
        else:
            runs.append([off, k, fi])
        off += k
    return runs


def exp_width(text, runs):
    w = 0.0
    for off, ln, fi in runs:
        seg = text.encode("utf-8")[off:off + ln].decode("utf-8")
        f = chain[fi]
        for ch in seg:
            if extender(ord(ch)):
                continue
            cm = cms[fi]
            g = cm.get(ord(ch))
            gid = f.getGlyphID(g) if g else 0
            w += f["hmtx"][f.getGlyphOrder()[gid]][0] * (100.0 / f["head"].unitsPerEm)
    return int(w * 1000 + 0.5)


def probe_runs(text):
    out = sh(["seg"] + FS + ["--", text]).decode().split("\n")
    runs = [[int(x) for x in l.split()[1:]] for l in out if l.startswith("run ")]
    wd = [int(l.split()[1]) for l in out if l.startswith("width ")][0]
    return runs, wd


if regex is None:
    print("  E SKIP: module `regex` not available -- runs / width NOT checked")
else:
    fixed = ["Ab", "Ab\U0001F600é", "❤️", "x❤y", "\U0001F468‍\U0001F469‍\U0001F467z",
             "\U0001F1E6\U0001F1F9\U0001F1E9\U0001F1EA", "\U0001F44D\U0001F3FD!", "éa", "1️⃣",
             "中文", "각각", "\U0001F3F4\U000E0067\U000E0062\U000E007F", "\r\n", ""]
    pool = list("ab é") + ["́", "‍", "️", "\U0001F600", "\U0001F468", "\U0001F469", "\U0001F3FD",
                                "\U0001F1E6", "\U0001F1F9", "❤", "★", "中", "⃣", "1", "가",
                                "ᄀ", "\U000E0067", "\U000E007F", "क", "्", "ष", "ำ"]
    texts = fixed + ["".join(rnd.choice(pool) for _ in range(rnd.randint(1, 10))) for _ in range(600)]
    bad = 0
    for t in texts:
        if not t:
            continue
        runs, wd = probe_runs(t)
        er = exp_runs(t)
        ew = exp_width(t, er)
        ok = runs == er[:64] and abs(wd - ew) <= 2
        if not ok:
            bad += 1
            if bad <= 4:
                print("   DIFF", t.encode("unicode_escape"), runs, er, wd, ew)
    check(bad == 0, "E %d texts differ" % bad)
    print("  E runs (cluster -> font) and width vs fontTools + regex \\X: %d texts, %d differ" % (len(texts), bad))

print("RESULT chain_check: %s (%d checks, %d failed)" % ("ok" if not fails else "FAIL", checks, fails))
sys.exit(1 if fails else 0)
