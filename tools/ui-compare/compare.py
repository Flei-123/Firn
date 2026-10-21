#!/usr/bin/env python3
"""Visual comparison: SSIM + mean absolute pixel deviation between PNGs.

Usage:
  compare.py A.png B.png              one pair (prints JSON)
  compare.py --table [--shots DIR]    table of all shots/<app>-<toolkit>-<theme>.png against the reference
                                      (flei: flei-original-<theme>.png, others: <app>-web-<theme>.png)
                                      writes results/compare.json and prints a text table.

SSIM: Gaussian-window SSIM (11x11, sigma 1.5, K1 .01, K2 .03) on grayscale luma, own numpy
implementation (same definition as skimage structural_similarity with gaussian_weights=True,
use_sample_covariance=False). Mean abs deviation: mean |a-b| over RGB, 0..255 and in percent of 255.
Images of different size: the other one is cropped/padded (white) to the reference size; reported in 'resized'.
"""
import json
import os
import sys

import numpy as np
from PIL import Image

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
HERE = os.environ.get("UIVGL_DIR", SCRIPTS)  # data/shots/results/apps root


def load(path):
    return Image.open(path).convert("RGB")


def _gauss_kernel(size=11, sigma=1.5):
    ax = np.arange(size) - size // 2
    k = np.exp(-(ax ** 2) / (2 * sigma ** 2))
    return k / k.sum()


def _filt(img, k):
    # separable correlation with 'valid' output
    r = len(k)
    h, w = img.shape
    tmp = np.zeros((h, w - r + 1))
    for i in range(r):
        tmp += k[i] * img[:, i:w - r + 1 + i]
    out = np.zeros((h - r + 1, w - r + 1))
    for i in range(r):
        out += k[i] * tmp[i:h - r + 1 + i, :]
    return out


def ssim_gray(a, b):
    a = a.astype(np.float64)
    b = b.astype(np.float64)
    k = _gauss_kernel()
    c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    mu_a, mu_b = _filt(a, k), _filt(b, k)
    saa = _filt(a * a, k) - mu_a ** 2
    sbb = _filt(b * b, k) - mu_b ** 2
    sab = _filt(a * b, k) - mu_a * mu_b
    s = ((2 * mu_a * mu_b + c1) * (2 * sab + c2)) / ((mu_a ** 2 + mu_b ** 2 + c1) * (saa + sbb + c2))
    return float(s.mean())


def compare(pa, pb):
    ia, ib = load(pa), load(pb)
    resized = False
    if ib.size != ia.size:
        resized = True
        canvas = Image.new("RGB", ia.size, (255, 255, 255))
        canvas.paste(ib, (0, 0))
        ib = canvas
    a = np.asarray(ia, dtype=np.float64)
    b = np.asarray(ib, dtype=np.float64)
    mad = float(np.abs(a - b).mean())
    luma = np.array([0.299, 0.587, 0.114])
    s = ssim_gray(a @ luma, b @ luma)
    return {"a": os.path.basename(pa), "b": os.path.basename(pb), "ssim": round(s, 4),
            "mad_255": round(mad, 2), "mad_pct": round(mad / 255 * 100, 2),
            "differing_px_pct": round(float((np.abs(a - b).max(axis=2) > 8).mean() * 100), 2),
            "resized": resized, "size": list(ia.size)}


def table(shots):
    rows = []
    names = sorted(f for f in os.listdir(shots) if f.endswith(".png"))
    for f in names:
        stem = f[:-4]
        if "detail" in stem:
            continue
        parts = stem.split("-")
        if len(parts) < 3:
            continue
        app, theme, toolkit = parts[0], parts[-1], "-".join(parts[1:-1])
        if theme not in ("light", "dark"):
            continue
        ref_name = "%s-%s-%s.png" % (app, "original" if app == "flei" else "web", theme)
        if f == ref_name:
            continue
        ref = os.path.join(shots, ref_name)
        if not os.path.exists(ref):
            rows.append({"app": app, "toolkit": toolkit, "theme": theme, "ref": ref_name, "error": "reference missing"})
            continue
        try:
            r = compare(ref, os.path.join(shots, f))
        except Exception as e:  # noqa: BLE001
            rows.append({"app": app, "toolkit": toolkit, "theme": theme, "ref": ref_name, "error": str(e)})
            continue
        r.update({"app": app, "toolkit": toolkit, "theme": theme, "ref": ref_name})
        rows.append(r)
    return rows


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        sys.exit(2)
    if args[0] == "--table":
        shots = os.path.join(HERE, "shots")
        if "--shots" in args:
            shots = args[args.index("--shots") + 1]
        rows = table(shots)
        os.makedirs(os.path.join(HERE, "results"), exist_ok=True)
        with open(os.path.join(HERE, "results", "compare.json"), "w") as f:
            json.dump(rows, f, indent=1)
        print("%-9s %-8s %-6s %-22s %7s %8s" % ("app", "toolkit", "theme", "reference", "SSIM", "MAD%"))
        for r in rows:
            if "error" in r:
                print("%-9s %-8s %-6s %-22s %s" % (r["app"], r["toolkit"], r["theme"], r["ref"], r["error"]))
            else:
                print("%-9s %-8s %-6s %-22s %7.4f %8.2f%s" % (r["app"], r["toolkit"], r["theme"], r["ref"],
                                                             r["ssim"], r["mad_pct"], " (resized)" if r["resized"] else ""))
        if not rows:
            print("(no shots yet)")
        return
    print(json.dumps(compare(args[0], args[1]), indent=1))


if __name__ == "__main__":
    main()
