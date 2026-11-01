#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import json
import math
import os
import subprocess
import sys
import tempfile

from PIL import Image


def read_ppm(path):
    return Image.open(path).convert("RGB")


def reference(pdf, page, workdir):
    stem = os.path.join(workdir, "ref")
    r = subprocess.run(["pdftoppm", "-r", "72", "-f", str(page + 1), "-l", str(page + 1), "-png", pdf, stem],
                       capture_output=True, timeout=60)
    if r.returncode != 0:
        return None
    for name in os.listdir(workdir):
        if name.startswith("ref") and name.endswith(".png"):
            path = os.path.join(workdir, name)
            image = Image.open(path).convert("RGB")
            os.remove(path)
            return image
    return None


def diff(a, b):
    if abs(a.size[0] - b.size[0]) > 1 or abs(a.size[1] - b.size[1]) > 1:
        return None
    w = min(a.size[0], b.size[0])
    h = min(a.size[1], b.size[1])
    a = a.crop((0, 0, w, h))
    b = b.crop((0, 0, w, h))
    ga = a.convert("L").tobytes()
    gb = b.convert("L").tobytes()
    total = 0
    sq = 0
    for x, y in zip(ga, gb):
        d = x - y
        total += abs(d)
        sq += d * d
    n = len(ga)
    mad = total / n
    mse = sq / n
    psnr = 99.0 if mse == 0 else 10 * math.log10(255 * 255 / mse)
    return mad, psnr


def render(binary, pdf, page, out):
    if os.environ.get("PDF_CMP_STYLE") == "file-first":
        argv = [binary, pdf, "render", out]
    else:
        argv = [binary, "render", pdf, str(page), "1", out]
    r = subprocess.run(argv, capture_output=True, timeout=120)
    return r.returncode == 0 and os.path.exists(out)


def main():
    binary = sys.argv[1]
    folder = sys.argv[2]
    page = 0
    results = []
    with tempfile.TemporaryDirectory() as work:
        for name in sorted(os.listdir(folder)):
            if not name.endswith(".pdf") or name.startswith("kaputt"):
                continue
            pdf = os.path.join(folder, name)
            out = os.path.join(work, "out.ppm")
            if os.path.exists(out):
                os.remove(out)
            ref = reference(pdf, page, work)
            ok = False
            try:
                ok = render(binary, pdf, page, out)
            except subprocess.TimeoutExpired:
                ok = False
            row = {"file": name, "ok": ok}
            if ref is not None and ok:
                mine = read_ppm(out)
                d = diff(mine, ref)
                row["size_ref"] = list(ref.size)
                row["size_mine"] = list(mine.size)
                if d:
                    row["mad"] = round(d[0], 3)
                    row["psnr"] = round(d[1], 2)
            results.append(row)
            print(json.dumps(row), flush=True)
    good = [r for r in results if "mad" in r]
    if good:
        print("files", len(results), "compared", len(good),
              "mean_mad", round(sum(r["mad"] for r in good) / len(good), 3),
              "mean_psnr", round(sum(r["psnr"] for r in good) / len(good), 2))


main()
