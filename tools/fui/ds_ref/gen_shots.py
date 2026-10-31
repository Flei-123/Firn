import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
TARGET = os.path.join(HERE, "..", "..", "..", "lib", "fui", "ds_tpl_shots.fi")
W = 296
H = 140


def frame():
    return [(0, 0, W, H, 6, "p"), (1, 1, W - 2, H - 2, 5, "s")]


def window():
    r = frame()
    r += [(10, 9, 8, 8, 4, "b"), (24, 9, 8, 8, 4, "b"), (38, 9, 8, 8, 4, "b"), (1, 26, W - 2, 1, 0, "p")]
    r += [(8, 34, 58, 98, 4, "r"), (14, 42, 46, 10, 3, "t"), (14, 60, 46, 8, 3, "p"), (14, 74, 46, 8, 3, "p"), (14, 88, 46, 8, 3, "p")]
    r += [(80, 36, 92, 12, 3, "b"), (80, 58, 200, 8, 3, "p"), (80, 72, 180, 8, 3, "p"), (80, 86, 150, 8, 3, "p")]
    r += [(80, 106, 64, 22, 4, "a"), (152, 106, 64, 22, 4, "p")]
    return r


def tiles():
    r = frame()
    r += [(12, 12, 90, 10, 3, "b"), (188, 8, 96, 18, 4, "r")]
    for row in range(2):
        for col in range(3):
            x = 12 + col * 94
            y = 36 + row * 52
            r += [(x, y, 88, 46, 6, "r"), (x + 8, y + 9, 28, 28, 7, "t" if (row + col) % 2 == 0 else "a"),
                  (x + 44, y + 12, 36, 8, 3, "b"), (x + 44, y + 26, 26, 8, 3, "p")]
    return r


def chart():
    r = frame()
    r += [(12, 12, 80, 10, 3, "b"), (12, 118, 272, 1, 0, "b")]
    heights = [40, 64, 52, 84, 60, 96, 72]
    for k, h in enumerate(heights):
        r.append((20 + k * 36, 118 - h, 24, h, 4, "a" if k == 5 else "t"))
    return r


def chat():
    r = frame()
    r += [(12, 10, 22, 22, 11, "t"), (42, 16, 70, 8, 3, "b")]
    r += [(12, 40, 150, 22, 10, "p"), (12, 66, 100, 22, 10, "p"), (134, 92, 150, 22, 10, "a")]
    r += [(12, 120, 272, 14, 7, "r")]
    return r


def encode(scenes):
    return "|".join(";".join(",".join(str(v) for v in rect[:5]) + "," + rect[5] for rect in s) for s in scenes)


def scenes():
    return [window(), tiles(), chart(), chat()]


def load():
    src = open(TARGET).read()
    m = re.search(r'const SCENES: str = "([^"]*)"', src)
    out = []
    for sc in m.group(1).split("|"):
        out.append([(int(p[0]), int(p[1]), int(p[2]), int(p[3]), int(p[4]), p[5]) for p in (r.split(",") for r in sc.split(";"))])
    return out


if __name__ == "__main__":
    data = encode(scenes())
    src = open(TARGET).read()
    src = re.sub(r'const SCENES: str = "[^"]*"', lambda m: 'const SCENES: str = "%s"' % data, src)
    open(TARGET, "w").write(src)
    print(len(data))
