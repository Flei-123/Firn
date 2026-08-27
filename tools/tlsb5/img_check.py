#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/tlsb5/img_check.py -- `<img>` in the layout and on the canvas.

TWO DIFFERENT KINDS OF MEASUREMENT are in this file and they are worth
different amounts, so they are reported separately:

  * THE GEOMETRY is checked against the sizing rules of CSS 2.1 10.3.2 and
    10.4 as this file writes them out. That is a reading of the standard,
    not an independent implementation, and it is therefore weaker than the
    numbers of rounds B2 and B3, which came from the Web Platform Tests.
    It is said here rather than left to be assumed.

  * THE PIXELS are checked against PILLOW. A picture drawn at its
    intrinsic size has to arrive on the canvas EXACTLY as it went in --
    not "close", not "on average": every one of the pixels of the picture,
    at the place the layout put it. That comparison has an independent
    other side and it is the one that would catch a decoder, a display
    list or a rasteriser that is quietly wrong.

The lazy-loading case is a counter-check and not a claim: the same page
twice, once with `loading="lazy"` and once without, and the lazy one has
to ask for FEWER pictures. A `loading` attribute that is read and ignored
passes every other test there is.
"""
import io
import os
import struct
import subprocess
import sys

from PIL import Image

UA = """
html { display: block } body { display: block; margin: 8px }
div { display: block } p { display: block; margin: 16px 0 }
img { display: inline-block }
"""

OK = 0
FAILS = []
COUNTER_OK = 0
COUNTER_TOTAL = 0


def note(title, good, detail="", counter=False):
    global OK, COUNTER_OK, COUNTER_TOTAL
    if counter:
        COUNTER_TOTAL += 1
    if good:
        OK += 1
        if counter:
            COUNTER_OK += 1
    else:
        FAILS.append((title, detail))


def u32(v):
    return struct.pack("<I", v)


def job(html, css, images, vw=800, vh=600, lazy_y=10 ** 9, png_out=True):
    b = u32(vw) + u32(vh)
    for part in (html.encode(), UA.encode(), css.encode(), b""):
        b += u32(len(part)) + part
    b += u32(3 if png_out else 2)
    b += u32(len(images))
    for url, data in images:
        b += u32(len(url)) + url.encode()
        b += u32(len(data)) + data
    b += u32(lazy_y)
    return b


def run(binary, *a, **kw):
    r = subprocess.run([binary], input=job(*a, **kw), capture_output=True,
                       timeout=120)
    rep = {}
    imgs = []
    for line in r.stderr.decode().split("\n"):
        if line.startswith("IMG "):
            imgs.append(tuple(int(x) for x in line.split()[1:]))
        elif " " in line:
            k, v = line.split(" ", 1)
            rep[k] = v.strip()
    return rep, imgs, r.stdout


def png_bytes(w, h, f):
    im = Image.new("RGB", (w, h))
    for x in range(w):
        for y in range(h):
            im.putpixel((x, y), f(x, y))
    b = io.BytesIO()
    im.save(b, "PNG")
    return im, b.getvalue()


def main():
    binary = sys.argv[1]
    pic, pic_png = png_bytes(40, 30, lambda x, y: ((x * 6) % 256,
                                                   (y * 8) % 256,
                                                   (x + y) % 256))
    jpg = io.BytesIO()
    pic.save(jpg, "JPEG", quality=95, subsampling=0)
    pic_jpg = jpg.getvalue()

    imgs = [("a.png", pic_png), ("a.jpg", pic_jpg)]

    # ---------------------------------------------- 1. the sizing rules
    cases = [
        ("intrinsic size, nothing said",
         '<img src="a.png">', "", (40, 30)),
        ("width given, the height follows the ratio",
         '<img src="a.png" style="width:80px">', "", (80, 60)),
        ("height given, the width follows the ratio",
         '<img src="a.png" style="height:15px">', "", (20, 15)),
        ("both given, the ratio is broken on purpose",
         '<img src="a.png" style="width:100px;height:10px">', "",
         (100, 10)),
        ("a JPEG has an intrinsic size too",
         '<img src="a.jpg">', "", (40, 30)),
        ("max-width takes the height with it",
         '<img src="a.png" style="max-width:20px">', "", (20, 15)),
        ("a picture that never arrived keeps the attribute size",
         '<img src="missing.png" width="120" height="60">', "", (120, 60)),
        ("padding and a border are outside the picture",
         '<img src="a.png" style="padding:5px;border:2px solid black">',
         "", (54, 44)),
        ("a percentage width against the containing block",
         '<div style="width:200px"><img src="a.png" '
         'style="width:50%"></div>', "", (100, 75)),
    ]
    for title, body, css, want in cases:
        rep, got, _ = run(binary, "<html><body>%s</body></html>" % body,
                          css, imgs)
        if not got:
            note(title, False, "no IMG line came back; report %s" % rep)
            continue
        note(title, (got[0][2], got[0][3]) == want,
             "%dx%d, expected %dx%d" % (got[0][2], got[0][3], *want))

    # a picture with no size at all is NOT a replaced box
    rep, got, _ = run(binary,
                      "<html><body><img src=\"missing.png\"></body></html>",
                      "", imgs)
    note("COUNTER-CHECK: a picture with no size and no attributes is not "
         "given one", len(got) == 0 and rep.get("IMAGES") == "0",
         "IMAGES %s, %d boxes" % (rep.get("IMAGES"), len(got)),
         counter=True)

    # ------------------------------------------- 2. the pixels, exactly
    rep, got, out = run(binary,
                        '<html><body style="margin:0">'
                        '<img src="a.png"></body></html>',
                        "html,body{margin:0;padding:0}", imgs,
                        vw=200, vh=120)
    if not got:
        note("the picture is painted", False, "no IMG box")
    else:
        canvas = Image.open(io.BytesIO(out)).convert("RGB")
        x, y, w, h = got[0]
        worst = 0
        for dx in range(40):
            for dy in range(30):
                a = canvas.getpixel((x + dx, y + dy))
                b = pic.getpixel((dx, dy))
                for k in range(3):
                    worst = max(worst, abs(a[k] - b[k]))
        note("the picture arrives on the canvas pixel for pixel",
             worst == 0, "worst channel difference %d at (%d,%d) %dx%d"
             % (worst, x, y, w, h))

    # scaled to twice the size: every source pixel must appear as a 2x2
    rep, got, out = run(binary,
                        '<html><body style="margin:0">'
                        '<img src="a.png" style="width:80px">'
                        '</body></html>',
                        "html,body{margin:0;padding:0}", imgs,
                        vw=200, vh=200)
    if got:
        canvas = Image.open(io.BytesIO(out)).convert("RGB")
        x, y, w, h = got[0]
        worst = 0
        for dx in range(80):
            for dy in range(60):
                a = canvas.getpixel((x + dx, y + dy))
                b = pic.getpixel((dx // 2, dy // 2))
                for k in range(3):
                    worst = max(worst, abs(a[k] - b[k]))
        note("scaled to twice the size, every source pixel is a 2x2 block",
             worst == 0, "worst channel difference %d" % worst)

    # THE COUNTER-CHECK OF K7B: is anything there at all? A canvas that is
    # entirely the background colour would pass a "mean difference" test
    # against a pale picture and fail this one.
    if got:
        canvas = Image.open(io.BytesIO(out)).convert("RGB")
        colours = set()
        for dx in range(80):
            for dy in range(60):
                colours.add(canvas.getpixel((x + dx, y + dy)))
        note("COUNTER-CHECK: the painted area is not one flat colour",
             len(colours) > 100, "%d distinct colours in the picture"
             % len(colours), counter=True)

    # ------------------------------------------------ 3. lazy loading
    many = "".join('<div style="height:300px"><img src="p%d.png" '
                   'width="40" height="30"></div>' % i for i in range(10))
    lazy = "".join('<div style="height:300px"><img src="p%d.png" '
                   'width="40" height="30" loading="lazy"></div>'
                   % i for i in range(10))
    rep_e, _, _ = run(binary, "<html><body>%s</body></html>" % many, "",
                      imgs, vw=800, vh=600, lazy_y=1200)
    rep_l, _, _ = run(binary, "<html><body>%s</body></html>" % lazy, "",
                      imgs, vw=800, vh=600, lazy_y=1200)
    eager = int(rep_e.get("PENDING", "0"))
    lazyn = int(rep_l.get("PENDING", "0"))
    note("every picture of the page is asked for when nothing is lazy",
         eager == 10, "PENDING %d" % eager)
    note("COUNTER-CHECK: loading=lazy really asks for fewer",
         lazyn < eager and lazyn > 0,
         "lazy %d against eager %d" % (lazyn, eager), counter=True)

    print("   %d / %d image cases, of them %d / %d counter-checks"
          % (OK, OK + len(FAILS), COUNTER_OK, COUNTER_TOTAL))
    for title, detail in FAILS:
        print("  FAIL  %-50s %s" % (title[:50], detail))
    if FAILS:
        print("IMG FAILED: %d" % len(FAILS))
        return 1
    print("IMG OK: %d cases, %d counter-checks" % (OK, COUNTER_OK))
    return 0


if __name__ == "__main__":
    sys.exit(main())
