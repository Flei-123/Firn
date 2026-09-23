#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/xwd2png.py -- ein Bildschirmfoto von `xwd` als PNG.
#
# WARUM: der Beleg fuer das echte Fenster kommt VOM SERVER (`xwd -root`
# bzw. `xwd -id <fenster>`), nicht aus dem Speicherpuffer des Programms --
# ein Programm, das nur behauptet, gemalt zu haben, faellt so auf. Auf
# diesem Rechner gibt es kein ImageMagick; PIL liest XWD nicht. Also steht
# der Kopf des XWD-Formats (X11R7, XWDFileHeader, 25 Worte grossendisch)
# hier selbst. Nur ZPixmap mit 24/32 Bit je Punkt -- das, was Xvfb -screen
# ...x24 liefert.
#
#     python3 tools/fui/xwd2png.py ein.xwd aus.png [x y w h]
import struct, sys
from PIL import Image

data = open(sys.argv[1], "rb").read()
kopf = struct.unpack(">25I", data[:100])
(hsize, version, fmt, depth, w, h, xoff, byte_order, bunit, bbo, bpad,
 bpp, bpl, vclass, rmask, gmask, bmask, bprgb, cmap_n, ncolors,
 ww, wh, wx, wy, bw) = kopf
if version != 7 or fmt != 2 or bpp not in (24, 32):
    raise SystemExit("nicht unterstuetzt: version %d format %d bpp %d"
                     % (version, fmt, bpp))
at = hsize + ncolors * 12
px = data[at:at + bpl * h]
bpp8 = bpp // 8


def schieb(m):
    s = 0
    while m and not (m & 1):
        m >>= 1
        s += 1
    return s


rs, gs, bs = schieb(rmask), schieb(gmask), schieb(bmask)
out = bytearray(w * h * 3)
for y in range(h):
    zeile = px[y * bpl:(y + 1) * bpl]
    for x in range(w):
        o = x * bpp8
        if byte_order == 0:
            v = int.from_bytes(zeile[o:o + bpp8], "little")
        else:
            v = int.from_bytes(zeile[o:o + bpp8], "big")
        i = (y * w + x) * 3
        out[i] = (v & rmask) >> rs
        out[i + 1] = (v & gmask) >> gs
        out[i + 2] = (v & bmask) >> bs
bild = Image.frombytes("RGB", (w, h), bytes(out))
if len(sys.argv) >= 7:
    x, y, cw, ch = (int(a) for a in sys.argv[3:7])
    bild = bild.crop((x, y, x + cw, y + ch))
bild.save(sys.argv[2])
print("%s: %dx%d, Tiefe %d, %d Bit je Punkt" % (sys.argv[2], w, h, depth, bpp))
