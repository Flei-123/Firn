#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/refdiff.py -- SCREENSHOT DIFF AGAINST CHECKED-IN GOLDENS.
#
# The gallery paints PNGs headless; this tool compares them with the
# reference images in tools/fui/golden/ and prints ONE number per image:
# the share of pixels that differ (any channel by more than --tol).
#
#     refdiff.py compare <actual_dir> [<golden_dir>] [--tol 8] [--max 0.5]
#         every *.png in golden_dir must exist in actual_dir with the same
#         size and a diff share <= --max percent; exit 1 otherwise.
#     refdiff.py update  <actual_dir> [<golden_dir>]
#         take the actual images as the new goldens (after a deliberate
#         design change -- review the diff images first).
#     refdiff.py diff <a.png> <b.png> <out.png>
#         red-highlighted difference image, prints the share.
#     refdiff.py pair <a.png> <b.png> [--tol 8]
#         one number: the diff share of two images.
#
# Last line of `compare`: `GOLDEN-DIFF: <worst>% worst, <mean>% mean, <n> images`.
import os
import shutil
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_GOLDEN = os.path.join(HERE, "golden")


def load(path):
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.int16)


def share(a, b, tol):
    """Percent of pixels whose largest channel delta exceeds tol."""
    if a.shape != b.shape:
        return 100.0
    d = np.abs(a - b).max(axis=2)
    return 100.0 * float((d > tol).sum()) / float(d.shape[0] * d.shape[1])


def opt(args, name, default):
    if name in args:
        i = args.index(name)
        v = args[i + 1]
        del args[i:i + 2]
        return type(default)(v)
    return default


def cmd_compare(args):
    tol = opt(args, "--tol", 8)
    mx = opt(args, "--max", 0.5)
    actual = args[0]
    golden = args[1] if len(args) > 1 else DEFAULT_GOLDEN
    names = sorted(n for n in os.listdir(golden) if n.endswith(".png"))
    if not names:
        print("refdiff: no goldens in %s" % golden)
        return 1
    bad = 0
    shares = []
    for n in names:
        ap = os.path.join(actual, n)
        if not os.path.exists(ap):
            print("  MISSING %s" % n)
            bad += 1
            continue
        a = load(ap)
        g = load(os.path.join(golden, n))
        if a.shape != g.shape:
            print("  SIZE    %s  actual %s golden %s" % (n, a.shape[:2][::-1], g.shape[:2][::-1]))
            bad += 1
            shares.append(100.0)
            continue
        s = share(a, g, tol)
        shares.append(s)
        ok = s <= mx
        if not ok:
            bad += 1
        print("  %-7s %-34s %7.3f %%" % ("ok" if ok else "DIFF", n, s))
    worst = max(shares) if shares else 100.0
    mean = sum(shares) / len(shares) if shares else 100.0
    print("GOLDEN-DIFF: %.3f%% worst, %.3f%% mean, %d images, %d over %.2f%%" %
          (worst, mean, len(names), bad, mx))
    return 1 if bad else 0


def cmd_update(args):
    actual = args[0]
    golden = args[1] if len(args) > 1 else DEFAULT_GOLDEN
    os.makedirs(golden, exist_ok=True)
    n = 0
    for f in sorted(os.listdir(actual)):
        if f.endswith(".png"):
            shutil.copyfile(os.path.join(actual, f), os.path.join(golden, f))
            n += 1
    print("refdiff: %d goldens written to %s" % (n, golden))
    return 0


def cmd_diff(args):
    tol = opt(args, "--tol", 8)
    a = load(args[0])
    b = load(args[1])
    if a.shape != b.shape:
        print("SIZE %s vs %s" % (a.shape[:2][::-1], b.shape[:2][::-1]))
        return 1
    d = np.abs(a - b).max(axis=2) > tol
    out = (a // 3 + 170).astype(np.uint8)
    out[d] = (255, 0, 0)
    Image.fromarray(out, "RGB").save(args[2])
    print("%.3f %%" % (100.0 * float(d.sum()) / float(d.size)))
    return 0


def cmd_pair(args):
    tol = opt(args, "--tol", 8)
    print("%.3f" % share(load(args[0]), load(args[1]), tol))
    return 0


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    cmd = sys.argv[1]
    rest = sys.argv[2:]
    fn = {"compare": cmd_compare, "update": cmd_update,
          "diff": cmd_diff, "pair": cmd_pair}.get(cmd)
    if fn is None:
        print(__doc__)
        return 2
    return fn(rest)


if __name__ == "__main__":
    sys.exit(main())
