import random
import subprocess

import numpy as np
from PIL import Image, ImageFilter


def enc(cli, kind, text):
    r = subprocess.run([cli, "enc", kind, text], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else None


def render(mods, scale=3, quiet=10, height=80, bg=255, fg=0):
    w = (len(mods) + 2 * quiet) * scale
    row = np.full(w, bg, dtype=np.uint8)
    for i, c in enumerate(mods):
        if c == "1":
            row[(quiet + i) * scale:(quiet + i + 1) * scale] = fg
    return Image.fromarray(np.tile(row, (height, 1)), "L")


def degrade(img, level, rng):
    if level == 0:
        return img
    if level >= 1:
        img = img.rotate(rng.uniform(-6, 6), expand=True, fillcolor=255, resample=Image.BILINEAR)
    if level >= 2:
        img = img.filter(ImageFilter.GaussianBlur(rng.uniform(0.4, 0.9)))
    if level >= 3:
        a = np.asarray(img).astype(np.float32)
        a = a * rng.uniform(0.55, 0.85) + rng.uniform(20, 60)
        a += np.random.default_rng(rng.randrange(1 << 30)).normal(0, 9, a.shape)
        img = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
    return img


def run_dec(cli, paths):
    r = subprocess.run([cli, "dec"] + paths, capture_output=True, text=True)
    out = {}
    for line in r.stdout.splitlines():
        p = line.split(" ", 1)
        out[p[0]] = p[1] if len(p) > 1 else ""
    return out
