#!/usr/bin/env python3
import io
import os
import sys
import zipfile

from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import layout_check as lc


def screens(uw, uh):
    y0 = (uh - 166) // 4
    cx = uw // 2
    return {
        "pause": [("back to game", cx - 102, y0 + 50, 204), ("options", cx - 102, y0 + 122, 98), ("disconnect", cx - 102, y0 + 146, 204)],
        "options": [("done", cx - 100, uh // 6 + 168, 200)],
        "controls": [("reset keys", cx - 155, uh - 29, 150), ("done", cx + 5, uh - 29, 150)],
    }


def main():
    ppm, jar, mode = sys.argv[1], sys.argv[2], sys.argv[3]
    w, h, raw = lc.read_ppm(ppm)
    img = Image.frombytes("RGB", (w, h), raw)
    with zipfile.ZipFile(jar) as z:
        tex = Image.open(io.BytesIO(z.read(lc.WIDGETS))).convert("RGBA")
    s = lc.scale_for(w, h)
    uw = (w + s - 1) // s
    uh = (h + s - 1) // s
    errors = []
    for name, x, y, bw in screens(uw, uh)[mode]:
        lc.check_button(img, tex, s, x, y, bw, errors, name)
    if errors:
        for e in errors:
            print("  FAIL  pause %s: %s" % (mode, e))
        sys.exit(1)
    print("  ok    pause %s: buttons match widgets.png" % mode)


main()
