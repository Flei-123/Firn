#!/usr/bin/env python3
"""pixcmp.py -- compare a fUi screenshot with a web reference (PNG vs PNG).

Per pair it prints ONE line with the numbers that matter for "does it look like the web app":
  diff     % of pixels whose largest channel difference is above --tol (default 24 of 255)
  ssim     structural similarity (7x7 box window, gray, 1.0 = identical)
  lines    text-line bands found (candidate / reference)
  dy       mean vertical offset of matched text-line bands in px (top edge), dx = same for the left edge
  score    100 * (0.5 * ssim + 0.5 * (1 - diff/100)); 100 = identical

Text-line bands: rows whose "ink" (strong local contrast) covers less than 60 % of the width are grouped into
runs; rows with longer ink (borders, bars) are skipped, so cards do not count as text. Bands are matched by order
(top to bottom), the count difference is reported separately.

usage:
  pixcmp.py A.png B.png [--out diff.png] [--tol 24] [--json]
  pixcmp.py --dir CAND_DIR REF_DIR [--suffix -dark] [--out-dir DIFFDIR]   (same file names in both dirs)
Images of different size: the candidate is compared on the common top-left area and a "size" warning is printed.
Needs Pillow + numpy.
"""
import argparse, json, os, sys

try:
    import numpy as np
    from PIL import Image
except ImportError:
    print("pixcmp: needs Pillow and numpy", file=sys.stderr)
    sys.exit(2)


def load(path):
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.int16)


def box_mean(a, k):
    """Mean over k x k windows (valid area) via cumulative sums."""
    c = np.cumsum(np.cumsum(a, axis=0), axis=1)
    c = np.pad(c, ((1, 0), (1, 0)))
    s = c[k:, k:] - c[:-k, k:] - c[k:, :-k] + c[:-k, :-k]
    return s / (k * k)


def ssim_gray(a, b, k=7):
    ga = a.astype(np.float64) @ np.array([0.299, 0.587, 0.114])
    gb = b.astype(np.float64) @ np.array([0.299, 0.587, 0.114])
    if min(ga.shape) < k:
        return 1.0 if np.array_equal(ga, gb) else 0.0
    ma, mb = box_mean(ga, k), box_mean(gb, k)
    saa = box_mean(ga * ga, k) - ma * ma
    sbb = box_mean(gb * gb, k) - mb * mb
    sab = box_mean(ga * gb, k) - ma * mb
    c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    m = ((2 * ma * mb + c1) * (2 * sab + c2)) / ((ma * ma + mb * mb + c1) * (saa + sbb + c2))
    return float(m.mean())


def ink_mask(img, thr=40):
    g = img.astype(np.float64) @ np.array([0.299, 0.587, 0.114])
    gx = np.abs(np.diff(g, axis=1, prepend=g[:, :1]))
    gy = np.abs(np.diff(g, axis=0, prepend=g[:1, :]))
    return (gx > thr) | (gy > thr)


def line_bands(img, max_cover=0.6, min_ink=2, gap=2, min_h=5, max_h=48):
    """Text-line bands [(top, bottom, left, right)] top to bottom."""
    m = ink_mask(img)
    h, w = m.shape
    cover = m.sum(axis=1)
    row_ok = (cover >= min_ink) & (cover <= max_cover * w)
    bands, y = [], 0
    while y < h:
        if not row_ok[y]:
            y += 1
            continue
        y0, last = y, y
        while y < h and (row_ok[y] or y - last <= gap):
            if row_ok[y]:
                last = y
            y += 1
        if min_h <= last - y0 + 1 <= max_h:
            cols = np.where(m[y0:last + 1].any(axis=0))[0]
            bands.append((y0, last, int(cols[0]), int(cols[-1])))
    return bands


def compare(pa, pb, tol=24, out=None):
    a, b = load(pa), load(pb)
    size_note = ""
    if a.shape != b.shape:
        size_note = "size %dx%d vs %dx%d" % (a.shape[1], a.shape[0], b.shape[1], b.shape[0])
        h, w = min(a.shape[0], b.shape[0]), min(a.shape[1], b.shape[1])
        a, b = a[:h, :w], b[:h, :w]
    d = np.abs(a - b).max(axis=2)
    diff_pct = 100.0 * float((d > tol).mean())
    mean_abs = float(np.abs(a - b).mean())
    ssim = ssim_gray(a, b)
    la, lb = line_bands(a), line_bands(b)
    n = min(len(la), len(lb))
    dy = float(np.mean([abs(la[i][0] - lb[i][0]) for i in range(n)])) if n else 0.0
    dx = float(np.mean([abs(la[i][2] - lb[i][2]) for i in range(n)])) if n else 0.0
    score = 100.0 * (0.5 * ssim + 0.5 * (1.0 - diff_pct / 100.0))
    if out:
        o = (b // 3 + 170).astype(np.uint8)  # pale reference as ground, differences in red
        o[d > tol] = (255, 0, 0)
        Image.fromarray(o).save(out)
    return {"diff_pct": round(diff_pct, 2), "ssim": round(ssim, 4), "mean_abs": round(mean_abs, 2),
            "lines_cand": len(la), "lines_ref": len(lb), "dy": round(dy, 2), "dx": round(dx, 2),
            "score": round(score, 1), "size": size_note}


def line(name, r):
    s = "%-22s diff=%5.1f%%  ssim=%.3f  lines=%d/%d  dy=%.1fpx dx=%.1fpx  score=%.1f" % (
        name, r["diff_pct"], r["ssim"], r["lines_cand"], r["lines_ref"], r["dy"], r["dx"], r["score"])
    return s + ("  [" + r["size"] + "]" if r["size"] else "")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("a", nargs="?")
    ap.add_argument("b", nargs="?")
    ap.add_argument("--tol", type=int, default=24)
    ap.add_argument("--out")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--dir", nargs=2, metavar=("CAND", "REF"))
    ap.add_argument("--suffix", default="")
    ap.add_argument("--out-dir")
    a = ap.parse_args()
    res = {}
    if a.dir:
        cand, ref = a.dir
        if a.out_dir:
            os.makedirs(a.out_dir, exist_ok=True)
        for n in sorted(os.listdir(cand)):
            if n.endswith(a.suffix + ".png") and os.path.isfile(os.path.join(ref, n)):
                o = os.path.join(a.out_dir, "diff-" + n) if a.out_dir else None
                res[n[:-4]] = compare(os.path.join(cand, n), os.path.join(ref, n), a.tol, o)
    elif a.a and a.b:
        res[os.path.basename(a.a)[:-4]] = compare(a.a, a.b, a.tol, a.out)
    else:
        ap.print_help()
        return 2
    if a.json:
        print(json.dumps(res, indent=1))
    else:
        for k, r in res.items():
            print(line(k, r))
    return 0


if __name__ == "__main__":
    sys.exit(main())
