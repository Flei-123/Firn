#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import sys

import numpy as np
from PIL import Image

lim, app, web = float(sys.argv[1]), sys.argv[2], sys.argv[3]
natives = sys.argv[4:]


def load(path):
    return np.asarray(Image.open(path).convert("RGB")).astype(int)


def diff_pct(a, b):
    h = min(a.shape[0], b.shape[0])
    w = min(a.shape[1], b.shape[1])
    d = np.abs(a[:h, :w] - b[:h, :w]).sum(axis=2)
    return 100.0 * float((d > 24).sum()) / (h * w), w, h


try:
    a = load(web)
    results = [(diff_pct(a, load(n)), n) for n in natives]
except OSError as e:
    print("RESULT %s NO-SCREENSHOT %s" % (app, e))
    sys.exit(1)
(pct, w, h), best = min(results, key=lambda r: r[0][0])
blank = int(a.reshape(-1, 3).std(axis=0).max() < 2.0)
ok = (not blank) and pct <= lim
print("RESULT %s %s differing_px_pct=%.2f blank=%d size=%dx%d ref=%s" % (app, "PASS" if ok else "FAIL", pct, blank, w, h, best.rsplit("-", 1)[-1][:-4]))
sys.exit(0 if ok else 1)
