import sys
from PIL import Image

HOTBAR_ITEM_X = 375
BAR_Y = 702
BAR_H = 6
SLOT_STRIDE = 60
BAR_SLOTS = [0, 2, 4, 6, 7]
MAX_BAD = 14


def load(path):
    return Image.open(path).convert("RGB")


def bar_bad(ref, got, slot):
    x0 = HOTBAR_ITEM_X + SLOT_STRIDE * slot + 6
    bad = 0
    for y in range(BAR_Y, BAR_Y + BAR_H):
        for x in range(x0, x0 + 39):
            a = ref.getpixel((x, y))
            b = got.getpixel((x, y))
            if max(abs(a[i] - b[i]) for i in range(3)) > 24:
                bad += 1
    return bad


INV_CELLS = [("inv0", 398, 362, 3000), ("hb0", 398, 536, 3000), ("hb4", 614, 536, 9000)]


def cell_diff(ref, got, x, y):
    total = 0
    for j in range(4, 48):
        for i in range(4, 48):
            a = ref.getpixel((x + i, y + j))
            b = got.getpixel((x + i, y + j))
            total += sum(abs(a[k] - b[k]) for k in range(3))
    return total


def main():
    d = sys.argv[1]
    ref, got = load(d + "/van_hotbar.png"), load(d + "/mc_hotbar.png")
    fails = 0
    for slot in BAR_SLOTS:
        bad = bar_bad(ref, got, slot)
        ok = bad <= MAX_BAD
        print("durability bar slot", slot, "bad_px", bad, "ok" if ok else "FAIL")
        if not ok:
            fails += 1
    ref_i, got_i = load(d + "/van_inv.png"), load(d + "/mc_inv.png")
    for name, x, y, limit in INV_CELLS:
        diff = cell_diff(ref_i, got_i, x, y)
        ok = diff <= limit
        print("inventory cell", name, "diff", diff, "ok" if ok else "FAIL")
        if not ok:
            fails += 1
    sys.exit(1 if fails else 0)


main()
