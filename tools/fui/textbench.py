#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/fui/textbench.py -- THE TEXT BENCH of fUi (W2, 17.10.2026).

Builds tools/fui/textbench_main.fi, paints one 40 character line at 13, 16 and
24 px (black on white, white on black, light on dark grey), with and without
the coverage curve (lib/fui/textcurve.fi), and measures against a reference
that does not come from fUi:

  fidelity   mean/max |coverage - reference| in percent, curve OFF, black on
             white. Reference = FreeType (Pillow) at 16x size, box-filtered
             down: exact unhinted area coverage. Pen = fUi's own pen.
  perceptual mean |pixel - ideal| in octets over the text box, where ideal =
             the reference coverage composited in LINEAR light (gamma 2.2),
             i.e. what a physically correct compositor shows. Curve off vs on,
             per colour pair.
  advance    run_width of the line against fontTools (hmtx + kern table), px.
  time       ns per 40 character line at 16 px: cold (coverage cache off, every
             line rastered) and warm (kept line written back). Best of N runs.

    python3 tools/fui/textbench.py [--font F.ttf] [--runs N] [--assert]

`--assert` turns the thresholds at the bottom into the exit code (used by
tools/fui/text_run.sh).
"""
import argparse, os, subprocess, sys, tempfile, shutil
import numpy as np
from PIL import Image, ImageFont, ImageDraw
from fontTools.ttLib import TTFont

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
LINE = "Hamburgefonstiv Quick fox AVATAR Tofu ry"
X0, BASE = 40, 60
SIZES = [13, 16, 24]
PAIRS = [((0, 0, 0), (255, 255, 255)),        # black on white
         ((255, 255, 255), (0, 0, 0)),        # white on black
         ((224, 224, 224), (30, 30, 30))]     # light on dark grey


def build(tmp):
    exe = os.path.join(tmp, "textbench")
    env = dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib"))
    firnc = os.environ.get("FIRNC", os.path.join(ROOT, "compiler/target/release/firnc"))
    if not os.path.exists(firnc):  # a worktree shares the compiler of /root/firn
        firnc = "/root/firn/compiler/target/release/firnc"
    subprocess.check_call([firnc, "--opt-level=release-fast", "-o", exe,
                           os.path.join(ROOT, "tools/fui/textbench_main.fi")], env=env)
    return exe


def run(exe, font, out, off):
    os.makedirs(out, exist_ok=True)
    args = [exe, font, out] + (["off"] if off else [])
    txt = subprocess.check_output(args).decode()
    res = {}
    for l in txt.splitlines():
        k, *v = l.split()
        res[k + (" " + v[0] if k == "width" else "")] = int(v[-1])
    return res


def pen_positions(font, size):
    """fUi's own pen: hmtx advance + kern pair, unrounded (fontTools)."""
    t = TTFont(font)
    cmap = t.getBestCmap()
    upem = t["head"].unitsPerEm
    hm = t["hmtx"]
    kt = {}
    if "kern" in t:
        for st in t["kern"].kernTables:
            kt.update(st.kernTable)
    g = [cmap[ord(c)] for c in LINE]
    pen, xs = 0.0, []
    for i, n in enumerate(g):
        xs.append(pen * size / upem)
        pen += hm[n][0]
        if i + 1 < len(g):
            pen += kt.get((n, g[i + 1]), 0)
    return xs


def ref_cov(font, size):
    """Coverage of LINE: FreeType at 16x, box filtered. Glyph by glyph at
    fUi's pen (Pillow's own layout would add its own idea of kerning)."""
    S = 16
    f = ImageFont.truetype(font, size * S, layout_engine=ImageFont.Layout.BASIC)
    im = Image.new("L", (640 * S, 120 * S), 0)
    d = ImageDraw.Draw(im)
    for ch, x in zip(LINE, pen_positions(font, size)):
        if ch != " ":
            d.text((round((X0 + x) * S), BASE * S), ch, font=f, fill=255, anchor="ls")
    a = np.asarray(im, dtype=np.float64) / 255.0
    return a.reshape(120, S, 640, S).mean(axis=(1, 3))


def load(out, size, k):
    return np.asarray(Image.open(os.path.join(out, "t_%d_%d.png" % (size, k))).convert("RGB"),
                      dtype=np.float64)


def lin(x):
    return (x / 255.0) ** 2.2


def ideal(cov, fg, bg):
    """Linear light compositing of the reference coverage, back to octets."""
    out = np.zeros(cov.shape + (3,))
    for c in range(3):
        l = cov * lin(fg[c]) + (1 - cov) * lin(bg[c])
        out[..., c] = 255.0 * np.power(l, 1 / 2.2)
    return out


def advance_ref(font, size):
    t = TTFont(font)
    cmap = t.getBestCmap()
    upem = t["head"].unitsPerEm
    hm = t["hmtx"]
    kt = {}
    if "kern" in t:
        for st in t["kern"].kernTables:
            kt.update(st.kernTable)
    g = [cmap[ord(c)] for c in LINE]
    w = 0.0
    for i, n in enumerate(g):
        w += hm[n][0]
        if i + 1 < len(g):
            w += kt.get((n, g[i + 1]), 0)
    return w * size / upem


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--font", default="/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--assert", dest="check", action="store_true")
    a = ap.parse_args()
    tmp = tempfile.mkdtemp(prefix="textbench-")
    try:
        exe = build(tmp)
        on = off = None
        cold, warm = [], []
        for i in range(a.runs):
            on = run(exe, a.font, os.path.join(tmp, "on"), False)
            off = run(exe, a.font, os.path.join(tmp, "off"), True)
            cold.append(min(on["ns_cold"], off["ns_cold"]))
            warm.append(min(on["ns_warm"], off["ns_warm"]))
        fails = []
        # ---- fidelity (curve off, black on white)
        print("fidelity (curve off, black on white): coverage vs FreeType 16x reference")
        for s in SIZES:
            ref = ref_cov(a.font, s)
            ours = 1.0 - load(os.path.join(tmp, "off"), s, 0)[..., 0] / 255.0
            band = slice(BASE - 2 * s, BASE + s)
            d = np.abs(ours[band] - ref[band])
            print("  %2d px: mean %.3f %%  (ink pixels only: %.2f %%)  max %.1f %%" % (
                s, 100 * d.mean(), 100 * d[ref[band] > 0.02].mean(), 100 * d.max()))
            if 100 * d[ref[band] > 0.02].mean() > 3.0:
                fails.append("fidelity %d px" % s)
        # ---- perceptual
        print("perceptual: mean |pixel - linear light ideal| in octets (lower = better)")
        tot_off = tot_on = 0.0
        for s in SIZES:
            ref = ref_cov(a.font, s)
            band = slice(BASE - 2 * s, BASE + s)
            row = []
            for k, (fg, bg) in enumerate(PAIRS):
                idl = ideal(ref, fg, bg)[band]
                eo = np.abs(load(os.path.join(tmp, "off"), s, k)[band] - idl).mean()
                en = np.abs(load(os.path.join(tmp, "on"), s, k)[band] - idl).mean()
                tot_off += eo
                tot_on += en
                row.append("%s %.3f -> %.3f" % (["b/w", "w/b", "l/d"][k], eo, en))
            print("  %2d px: " % s + " | ".join(row))
        print("  sum over 9 cases: off %.3f  on %.3f  (%.0f %% of the error left)" % (
            tot_off, tot_on, 100 * tot_on / tot_off))
        if tot_on >= tot_off:
            fails.append("curve does not get closer to the ideal")
        # ---- advance
        print("advance: run_width vs fontTools (hmtx + kern), px")
        for s in SIZES:
            got = on["width %d" % s] / 1000.0
            want = advance_ref(a.font, s)
            print("  %2d px: fUi %.3f  ref %.3f  diff %.4f" % (s, got, want, got - want))
            if abs(got - want) > 0.01:
                fails.append("advance %d px" % s)
        # ---- kerning in the bidi path
        bk = on["bidikern"]
        print("bidi kerning: width('AVATAR To'+alef) - alef - width('AVATAR To') = %d / 1e6 px (must be 0)" % bk)
        if abs(bk) > 10:
            fails.append("bidi kerning")
        # ---- time
        print("time (best of %d), 40 chars at 16 px: cold %.0f us, warm %.0f us per line" % (
            a.runs, min(cold) / 1000.0, min(warm) / 1000.0))
        print("RESULT cold_us=%.1f warm_us=%.1f" % (min(cold) / 1000.0, min(warm) / 1000.0))
        if a.check:
            if fails:
                print("TEXTBENCH FAILED: " + ", ".join(fails))
                return 1
            print("TEXTBENCH PASSED.")
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
