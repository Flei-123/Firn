#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/paintb3/font_check.py -- the counter-check for the TrueType reader
and the glyph rasteriser of round B3.

Nothing here trusts a single number this repository produced.

  * the METRICS (units per em, ascent, descent, glyph id per character,
    advance width, side bearing, bounding box, kerning pair) are compared
    against **fontTools**, an implementation nobody here wrote, glyph for
    glyph over the whole font.

  * the PIXELS are compared against a second rasteriser written here in
    Python, and written on purpose with a COMPLETELY DIFFERENT ALGORITHM:
    a textbook scanline crossing test with a winding counter, exact in x
    and sampled 16 times per pixel in y.  `lib/font/raster.fi` walks each
    edge once and accumulates signed AREAS; the reference asks, for 16
    horizontal lines through every pixel row, which x intervals lie inside
    the non-zero winding.  The two share no line of code and no idea, and
    the outline they are both fed comes out of **fontTools** -- a
    TrueType parser nobody here wrote, which decomposes the composite
    glyphs as well.

    (An earlier version of this file used matplotlib's
    `Path.contains_points` as the reference.  It was thrown out because it
    is WRONG for this job: matplotlib treats the subpaths of a compound
    path as a union, so the counter of an `O` comes back filled.  It said
    the engine was wrong about `O`, `D`, `Q`, `©` and `®`, and the engine
    was right.  A yardstick has to be checked too.)

THE WARNING FROM ROUND K7B IN THE KERNEL is what shapes the text metric
here.  There, a screen was "87 per cent correct" while every single letter
was missing -- the 87 per cent were the black background.  So this file
never divides by the area of the image.  It counts, PER GLYPH:

    ink_firn      pixels this engine set above half coverage
    ink_ref       pixels the reference set above half coverage
    mad           the mean absolute difference of the COVERAGE over the
                  glyph window -- never over the page
    maxdiff       the LARGEST difference of a single pixel
    IoU           intersection over union of the two ink sets

and a glyph with zero ink where the reference has ink is a FAILURE, not a
rounding difference.  A glyph that is empty in both (the space) is counted
in its own column and never in the quota.

WHY `mad`/`maxdiff` DECIDE AND NOT `IoU`.  IoU counts pixels above half
coverage, so a vertical stem whose edge falls exactly on 0.500 flips a
whole column from "in" to "out" for a difference of one part in a hundred.
Greek beta at 48 px does exactly that: its stem edge sits on a pixel
boundary, its largest single-pixel deviation is 0.049 and its IoU is 0.92.
Judging by IoU would call that glyph broken; judging by the coverage says
what it is.  IoU is still printed, because a glyph that is really drawn in
the wrong place has a bad IoU and a bad MAD together.
"""
import argparse
import struct
import subprocess
import sys

import numpy as np
from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.ttLib import TTFont

# How many scan lines the reference puts through one pixel row.  In x it
# does not sample at all -- it integrates the interval exactly.
SUB = 16


def run_font(binary, font_bytes, script):
    job = struct.pack("<I", len(font_bytes)) + font_bytes + script
    p = subprocess.run([binary], input=job, capture_output=True)
    if p.returncode != 0:
        raise SystemExit("fontb3 failed (%d): %s" % (p.returncode, p.stderr[:400]))
    return p.stdout


def parse_out(raw):
    """The answer is text with raw octet blocks in it, so it is walked by
    hand rather than split on newlines."""
    out = []
    i = 0
    n = len(raw)
    while i < n:
        j = raw.find(b"\n", i)
        if j < 0:
            break
        line = raw[i:j]
        i = j + 1
        if not line:
            continue
        if line.startswith(b"T "):
            _, w, h, cnt = line.split()
            w, h, cnt = int(w), int(h), int(cnt)
            data = raw[i:i + cnt]
            i += cnt + 1
            out.append(("T", w, h, np.frombuffer(data, dtype=np.uint8).reshape(h, w)))
        else:
            out.append(tuple(line.decode().split()))
    return out


# ------------------------------------------------------------ the reference

def flatten(pen_value, steps=24):
    """The recorded outline as a list of closed polygons, in font units."""
    polys = []
    cur = []
    start = None
    pos = (0.0, 0.0)
    for op, args in pen_value:
        if op == "moveTo":
            if len(cur) > 2:
                polys.append(cur)
            pos = args[0]
            start = pos
            cur = [pos]
        elif op == "lineTo":
            pos = args[0]
            cur.append(pos)
        elif op == "qCurveTo":
            pts = list(args)
            # A trailing None means the contour is all off-curve points and
            # closes on the implied midpoint -- the TrueType special case.
            if pts[-1] is None:
                pts = pts[:-1]
                implied = ((pts[0][0] + pts[-1][0]) / 2.0,
                           (pts[0][1] + pts[-1][1]) / 2.0)
                pts = pts + [implied]
            on = pts[-1]
            offs = pts[:-1]
            prev = pos
            for k, c in enumerate(offs):
                if k + 1 < len(offs):
                    nxt = ((c[0] + offs[k + 1][0]) / 2.0,
                           (c[1] + offs[k + 1][1]) / 2.0)
                else:
                    nxt = on
                for s in range(1, steps + 1):
                    t = s / steps
                    mt = 1 - t
                    cur.append((mt * mt * prev[0] + 2 * mt * t * c[0] + t * t * nxt[0],
                                mt * mt * prev[1] + 2 * mt * t * c[1] + t * t * nxt[1]))
                prev = nxt
            pos = on
        elif op == "curveTo":
            c1, c2, p3 = args
            prev = pos
            for s in range(1, steps + 1):
                t = s / steps
                mt = 1 - t
                cur.append((mt ** 3 * prev[0] + 3 * mt * mt * t * c1[0]
                            + 3 * mt * t * t * c2[0] + t ** 3 * p3[0],
                            mt ** 3 * prev[1] + 3 * mt * mt * t * c1[1]
                            + 3 * mt * t * t * c2[1] + t ** 3 * p3[1]))
            pos = p3
        elif op == "closePath":
            if len(cur) > 2:
                polys.append(cur)
            cur = []
            pos = start
    if len(cur) > 2:
        polys.append(cur)
    return polys


def reference_bitmap(polys, w, h, penx, peny, scale):
    """The second rasteriser: scanline crossings, a winding counter, exact
    in x, SUB samples per pixel row in y.  Returns coverage 0..1."""
    cov = np.zeros((h, w), dtype=np.float64)
    if not polys:
        return cov
    ex0 = []
    ey0 = []
    ex1 = []
    ey1 = []
    for poly in polys:
        pts = [(penx + x * scale, peny - y * scale) for (x, y) in poly]
        pts.append(pts[0])
        for i in range(len(pts) - 1):
            (ax, ay), (bx, by) = pts[i], pts[i + 1]
            if ay == by:
                continue
            ex0.append(ax)
            ey0.append(ay)
            ex1.append(bx)
            ey1.append(by)
    if not ex0:
        return cov
    ex0 = np.array(ex0)
    ey0 = np.array(ey0)
    ex1 = np.array(ex1)
    ey1 = np.array(ey1)
    ylo = np.minimum(ey0, ey1)
    yhi = np.maximum(ey0, ey1)
    sign = np.where(ey1 > ey0, 1, -1)
    slope = (ex1 - ex0) / (ey1 - ey0)
    share = 1.0 / SUB
    for py in range(h):
        for s in range(SUB):
            y = py + (s + 0.5) / SUB
            sel = (ylo <= y) & (yhi > y)
            if not sel.any():
                continue
            xs = ex0[sel] + (y - ey0[sel]) * slope[sel]
            sg = sign[sel]
            order = np.argsort(xs, kind="stable")
            xs = xs[order]
            sg = sg[order]
            wind = 0
            start = 0.0
            for i in range(len(xs)):
                if wind != 0:
                    _add_span(cov[py], start, xs[i], share, w)
                wind += sg[i]
                start = xs[i]
    return cov


def _add_span(row, a, b, share, w):
    """The exact overlap of the interval [a, b) with every pixel column."""
    if b <= a:
        return
    if b <= 0 or a >= w:
        return
    a = max(a, 0.0)
    b = min(b, float(w))
    c0 = int(np.floor(a))
    c1 = int(np.ceil(b))
    for c in range(c0, min(c1, w)):
        lo = max(a, c)
        hi = min(b, c + 1.0)
        if hi > lo:
            row[c] += (hi - lo) * share


# ------------------------------------------------------------------- main

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("binary")
    ap.add_argument("font")
    ap.add_argument("--size", type=float, default=48.0)
    ap.add_argument("--iou", type=float, default=0.97,
                    help="reported only, does not decide")
    ap.add_argument("--max-mad", type=float, default=0.01,
                    help="mean deviation of the coverage per glyph")
    ap.add_argument("--max-pixel", type=float, default=0.20,
                    help="largest deviation of a single pixel per glyph")
    ap.add_argument("--json", default=None)
    args = ap.parse_args()

    font_bytes = open(args.font, "rb").read()
    tt = TTFont(args.font)
    upem = tt["head"].unitsPerEm
    cmap = tt.getBestCmap()
    order = tt.getGlyphOrder()
    hmtx = tt["hmtx"]
    glyf = tt["glyf"]
    gs = tt.getGlyphSet()

    codes = sorted(cmap.keys())

    # ---------------------------------------------------------- 1. metrics
    script = b"M\n"
    for cp in codes:
        script += b"G %d\n" % cp
    kern_pairs = []
    if "kern" in tt:
        kt = tt["kern"].kernTables[0].kernTable
        rev = {}
        for cp, name in cmap.items():
            rev.setdefault(name, cp)
        for (l, r), v in sorted(kt.items()):
            if l in rev and r in rev:
                kern_pairs.append((rev[l], rev[r], v))
        for a, b, _ in kern_pairs:
            script += b"K %d %d\n" % (a, b)

    res = parse_out(run_font(args.binary, font_bytes, script))
    it = iter(res)
    head = next(it)
    metric_bad = []
    if head[0] != "M":
        raise SystemExit("no metrics line")
    got = [int(x) for x in head[1:]]
    want = [upem, tt["hhea"].ascent, -tt["hhea"].descent, tt["hhea"].lineGap,
            tt["maxp"].numGlyphs]
    if got[:5] != want:
        metric_bad.append("font header %s != %s" % (got[:5], want))

    glyph_checked = 0
    for cp in codes:
        row = next(it)
        assert row[0] == "G", row
        _, rcp, gid, adv, lsb, has, x0, y0, x1, y1 = row
        name = cmap[cp]
        wgid = order.index(name)
        wadv, wlsb = hmtx[name]
        g = glyf[name]
        if g.numberOfContours == 0:
            wbox = None
        else:
            wbox = (g.xMin, g.yMin, g.xMax, g.yMax)
        gotbox = (int(x0), int(y0), int(x1), int(y1)) if has == "1" else None
        if (int(gid), int(adv), int(lsb), gotbox) != (wgid, wadv, wlsb, wbox):
            metric_bad.append("U+%04X: %s != %s" % (
                cp, (int(gid), int(adv), int(lsb), gotbox),
                (wgid, wadv, wlsb, wbox)))
        glyph_checked += 1

    kern_checked = 0
    for a, b, v in kern_pairs:
        row = next(it)
        assert row[0] == "K", row
        if int(row[1]) != v:
            metric_bad.append("kern U+%04X U+%04X: %s != %d" % (a, b, row[1], v))
        kern_checked += 1

    # ----------------------------------------------------------- 2. pixels
    size = args.size
    scale = size / upem
    drawable = []
    for cp in codes:
        name = cmap[cp]
        if glyf[name].numberOfContours == 0:
            continue
        drawable.append(cp)

    # One window per glyph, big enough for the whole outline plus a margin.
    script = b""
    windows = {}
    for cp in drawable:
        name = cmap[cp]
        g = glyf[name]
        g.recalcBounds(glyf)
        x0 = int(np.floor(g.xMin * scale)) - 2
        y1 = int(np.ceil(g.yMax * scale)) + 2
        w = int(np.ceil(g.xMax * scale)) + 2 - x0
        h = y1 - int(np.floor(g.yMin * scale)) + 2
        penx = -x0
        peny = y1
        windows[cp] = (w, h, penx, peny)
        script += b"T %d %d %d %d %d 0 1 %d\n" % (
            round(size * 1000), w, h, penx * 1000, peny * 1000, cp)

    res = parse_out(run_font(args.binary, font_bytes, script))
    bitmaps = [r for r in res if r[0] == "T"]
    assert len(bitmaps) == len(drawable), (len(bitmaps), len(drawable))

    rows = []
    for cp, bm in zip(drawable, bitmaps):
        _, w, h, arr = bm
        wref, href, penx, peny = windows[cp]
        assert (w, h) == (wref, href)
        pen = DecomposingRecordingPen(gs)
        gs[cmap[cp]].draw(pen)
        polys = flatten(pen.value, steps=48)
        ref = reference_bitmap(polys, w, h, penx, peny, scale)
        mine = arr.astype(np.float64) / 255.0
        a = mine > 0.5
        b = ref > 0.5
        inter = int(np.logical_and(a, b).sum())
        union = int(np.logical_or(a, b).sum())
        iou = inter / union if union else 1.0
        diff = np.abs(mine - ref)
        mad = float(diff.mean())
        mx = float(diff.max()) if diff.size else 0.0
        rows.append(dict(cp=cp, ink=int(a.sum()), ink_ref=int(b.sum()),
                         iou=iou, mad=mad, maxdiff=mx))

    empty = [r for r in rows if r["ink"] == 0]
    missing = [r for r in rows if r["ink"] == 0 and r["ink_ref"] > 0]
    good = [r for r in rows
            if r["mad"] <= args.max_mad and r["maxdiff"] <= args.max_pixel]
    worst = sorted(rows, key=lambda r: -r["maxdiff"])[:5]

    print("   font          %s" % args.font)
    print("   metrics       %d characters, %d kerning pairs against fontTools"
          % (glyph_checked, kern_checked))
    if metric_bad:
        print("   METRICS WRONG %d:" % len(metric_bad))
        for m in metric_bad[:10]:
            print("      %s" % m)
    else:
        print("   metrics       0 deviations")
    print("   glyphs drawn  %d of %d stay under MAD %.3f and %.2f in a "
          "single pixel against the reference rasteriser"
          % (len(good), len(rows), args.max_mad, args.max_pixel))
    print("   empty glyphs  %d drawn empty, of those %d WRONGLY empty"
          % (len(empty), len(missing)))
    if rows:
        print("   mean MAD      %.5f   mean IoU %.4f   worst pixel %.3f"
              % (float(np.mean([r["mad"] for r in rows])),
                 float(np.mean([r["iou"] for r in rows])),
                 float(max(r["maxdiff"] for r in rows))))
        print("   weakest       %s" % ", ".join(
            "U+%04X %.3f" % (r["cp"], r["maxdiff"]) for r in worst))
    print("FONT: %d / %d glyphs, %d metric deviations, %d wrongly empty"
          % (len(good), len(rows), len(metric_bad), len(missing)))

    if args.json:
        import json
        json.dump(dict(rows=rows, metric_bad=metric_bad,
                       good=len(good), total=len(rows),
                       missing=len(missing)), open(args.json, "w"))

    if metric_bad or missing:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
