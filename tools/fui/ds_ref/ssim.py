#!/usr/bin/env python3
import json
import sys

import numpy as np
from PIL import Image


def gray(img):
    a = np.asarray(img.convert("RGB"), dtype=np.float64)
    return 0.299 * a[..., 0] + 0.587 * a[..., 1] + 0.114 * a[..., 2]


def window(size=11, sigma=1.5):
    ax = np.arange(size) - size // 2
    k = np.exp(-(ax ** 2) / (2 * sigma ** 2))
    return k / k.sum()


def blur(img, k):
    r = len(k)
    h, w = img.shape
    tmp = np.zeros((h, w - r + 1))
    for i in range(r):
        tmp += k[i] * img[:, i:i + w - r + 1]
    out = np.zeros((h - r + 1, w - r + 1))
    for i in range(r):
        out += k[i] * tmp[i:i + h - r + 1, :]
    return out


def ssim(a, b):
    k = window()
    c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    ma, mb = blur(a, k), blur(b, k)
    saa = blur(a * a, k) - ma * ma
    sbb = blur(b * b, k) - mb * mb
    sab = blur(a * b, k) - ma * mb
    m = ((2 * ma * mb + c1) * (2 * sab + c2)) / ((ma * ma + mb * mb + c1) * (saa + sbb + c2))
    return float(m.mean())


def main():
    if len(sys.argv) < 3:
        raise SystemExit("usage: ssim.py REF.png SHOT.png [diff_out.png]")
    ref = Image.open(sys.argv[1]).convert("RGB")
    shot = Image.open(sys.argv[2]).convert("RGB")
    if ref.size != shot.size:
        print(json.dumps({"ssim": 0.0, "error": "size", "ref": ref.size, "shot": shot.size}))
        raise SystemExit(1)
    if len(sys.argv) > 3:
        d = np.abs(np.asarray(ref, dtype=np.int16) - np.asarray(shot, dtype=np.int16)).max(axis=2)
        Image.fromarray((255 - np.clip(d * 4, 0, 255)).astype(np.uint8)).save(sys.argv[3])
    print(json.dumps({"ssim": round(ssim(gray(ref), gray(shot)), 4)}))


if __name__ == "__main__":
    main()
