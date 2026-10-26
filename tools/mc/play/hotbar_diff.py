import sys
import numpy as np
from PIL import Image

SLOT_STRIDE = 60
HOTBAR_X = 368
HOTBAR_Y = 655
SEARCH = 6
MAX_SUM = 120000


def load(path):
    return np.array(Image.open(path).convert("RGB")).astype(int)


def best_shift(ref, got, slot):
    x0 = HOTBAR_X + 9 + SLOT_STRIDE * slot
    y0 = HOTBAR_Y + 9
    a = ref[y0:y0 + 48, x0:x0 + 42]
    best = None
    for dy in range(-SEARCH, SEARCH + 1):
        for dx in range(-SEARCH, SEARCH + 1):
            d = int(np.abs(a - got[y0 + dy:y0 + dy + 48, x0 + dx:x0 + dx + 42]).sum())
            if best is None or d < best[0]:
                best = (d, dx, dy)
    zero = int(np.abs(a - got[y0:y0 + 48, x0:x0 + 42]).sum())
    return best, zero


def main():
    ref, got = load(sys.argv[1]), load(sys.argv[2])
    slots = [int(v) for v in sys.argv[3:]] or [0]
    bad = 0
    for s in slots:
        best, zero = best_shift(ref, got, s)
        ok = best[1] == 0 and best[2] == 0 and zero <= MAX_SUM
        print("slot", s, "best", best, "at_zero", zero, "ok" if ok else "FAIL")
        if not ok:
            bad += 1
    sys.exit(1 if bad else 0)


main()
