#!/usr/bin/env python3
import sys

from PIL import Image


def main():
    img = Image.open(sys.argv[1]).convert("RGB")
    w, h = img.size
    s = max(1, min(4, min(w // 320, h // 240)))
    uw = (w + s - 1) // s
    left = uw // 2 - 152
    x0, x1 = (left + 305 - 45) * s, (left + 305 - 3) * s
    y0, y1 = 37 * s, 47 * s
    gray = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            if img.getpixel((x, y)) == (128, 128, 128):
                gray += 1
    if gray < 10:
        print("  FAIL  ping: no online/max text in the first server row (%d gray pixels)" % gray)
        sys.exit(1)
    print("  ok    ping: online/max text drawn (%d gray pixels)" % gray)


main()
