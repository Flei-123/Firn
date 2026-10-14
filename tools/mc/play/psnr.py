#!/usr/bin/env python3
"""GL vs CPU picture of the same mcplay scene: PSNR + per-region PSNR + differing-pixel share.

usage: psnr.py <cpu.png> <gl.png> [--min-db D] [--diff out.png] [--hud-rows N] [--mask x0,y0,x1,y1]

--mask blanks a rectangle (the F3 text: its FPS number differs by design). The HUD (crosshair / hotbar) is drawn by the same code in both paths but over a different background, so it is part of the
comparison; --hud-rows N cuts the bottom N rows (hotbar) when set. Regions: whole picture, upper half (sky/horizon),
lower half (terrain/water), and the bounding box of the pixels that differ most. Exit 1 when the PSNR is below --min-db.
"""
import sys

import numpy as np
from PIL import Image


def psnr(a, b):
    mse = float(np.mean((a.astype(np.float64) - b.astype(np.float64)) ** 2))
    return 99.0 if mse < 1e-12 else 10.0 * np.log10(255.0 * 255.0 / mse)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    opt = {}
    for k in ("--min-db", "--diff", "--hud-rows", "--mask"):
        if k in sys.argv:
            opt[k] = sys.argv[sys.argv.index(k) + 1]
            args = [a for a in args if a != opt[k]]
    cpu = np.asarray(Image.open(args[0]).convert("RGB"))
    gl = np.asarray(Image.open(args[1]).convert("RGB"))
    if cpu.shape != gl.shape:
        print(f"  psnr FAIL size mismatch {cpu.shape} vs {gl.shape}")
        return 1
    h = cpu.shape[0]
    cut = int(opt.get("--hud-rows", "0"))
    if cut > 0:
        cpu, gl = cpu[: h - cut], gl[: h - cut]
        h = cpu.shape[0]
    if "--mask" in opt:
        x0, y0, x1, y1 = (int(v) for v in opt["--mask"].split(","))
        gl = gl.copy()
        gl[y0:y1, x0:x1] = cpu[y0:y1, x0:x1]  # e.g. the debug text (FPS differs by design)
    whole = psnr(cpu, gl)
    top = psnr(cpu[: h // 2], gl[: h // 2])
    bot = psnr(cpu[h // 2:], gl[h // 2:])
    d = np.abs(cpu.astype(np.int32) - gl.astype(np.int32)).max(axis=2)
    diff_px = float(np.mean(d > 24)) * 100.0
    big = float(np.mean(d > 64)) * 100.0
    print(f"  psnr whole={whole:.2f}dB top={top:.2f}dB bottom={bot:.2f}dB differing(>24)={diff_px:.2f}% differing(>64)={big:.2f}% size={cpu.shape[1]}x{cpu.shape[0]}")
    ys, xs = np.nonzero(d > 64)
    if len(ys):
        print(f"  psnr big-diff bbox x={xs.min()}..{xs.max()} y={ys.min()}..{ys.max()}")
    if "--diff" in opt:
        Image.fromarray(np.clip(d * 3, 0, 255).astype(np.uint8)).save(opt["--diff"])
    if "--min-db" in opt and whole < float(opt["--min-db"]):
        print(f"  psnr FAIL {whole:.2f} dB < {opt['--min-db']} dB")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
