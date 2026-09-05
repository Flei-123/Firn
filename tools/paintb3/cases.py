#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/paintb3/cases.py -- the OWN cases of round B3.

Seven pages that between them touch every drawing command the engine has:
plain and rounded rectangles, eight border styles, linear and radial
gradients with `hsl()` in them, outer and inset box shadows, a text
shadow, real proportional text with kerning and a line break, the paint
order with `z-index` and a float, `overflow: hidden`, `opacity` and a
blend mode.

WHAT IS CHECKED, and it is three different things:

  1. THE PICTURE IS THE SAME AS LAST TIME.  Every case has a frozen
     SHA-256 in `expected.txt`.  A change to the rasteriser that moves one
     pixel shows up here immediately, which is exactly what a frozen
     expectation is for -- and when the change is meant, the hash is
     updated with `--freeze` and the commit says why.
  2. THE THREE BUILD STAGES AGREE.  `opt`, `--no-opt` and `dev-fast` have
     to produce the SAME OCTETS.  This is the strongest check in the file:
     the rasteriser is thousands of floating point operations per pixel
     deep, and an optimiser that reorders one of them would show up as a
     different picture and nowhere else.
  3. NOTHING IS EMPTY, AND IT IS MEASURED IN NUMBERS THAT WERE MEASURED.
     `expected.txt` carries, per case, the hash AND two counts: how many
     octets of the picture are not white, and how many pixels the glyph
     rasteriser really set.  Both have to stay within half of the frozen
     value.  That is the round K7B guard: there, a screen was called 87
     per cent correct while every letter was missing, because the 87 per
     cent were the background.  A hash alone would not have caught it --
     whoever re-froze the hash would have frozen the empty picture with
     it.  A glyph count of zero on a page with text cannot be re-frozen
     without the number in this file changing, in the commit, visibly.
"""
import argparse
import glob
import hashlib
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
HERE = os.path.dirname(os.path.abspath(__file__))


def blk(b):
    return struct.pack("<I", len(b)) + b


def render(binary, html, ua, font, vw, vh):
    j = (struct.pack("<II", vw, vh) + blk(html) + blk(ua) + blk(b"")
         + blk(font) + struct.pack("<I", 2))
    p = subprocess.run([binary], input=j, capture_output=True, timeout=180)
    if p.returncode != 0:
        return None, {}
    rep = {}
    for line in p.stderr.decode().splitlines():
        k = line.split()
        if len(k) == 2:
            rep[k[0]] = int(k[1])
    return p.stdout, rep


def painted(ppm):
    """How many octets of the picture are not white. Cheap and enough: the
    question is only whether anything was drawn at all."""
    i = ppm.index(b"255\n") + 4
    body = ppm[i:]
    return sum(1 for b in body if b != 255)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("binaries", nargs="+", help="one per build stage")
    ap.add_argument("--freeze", action="store_true")
    ap.add_argument("--width", type=int, default=400)
    ap.add_argument("--height", type=int, default=400)
    args = ap.parse_args()

    ua = open(os.path.join(ROOT, "lib/dom/ua.css"), "rb").read()
    font = open(os.path.join(ROOT, "tests/data/fonts/FirnSans.ttf"), "rb").read()
    cases = sorted(glob.glob(os.path.join(HERE, "cases", "*.html")))
    expfile = os.path.join(HERE, "cases", "expected.txt")
    expected = {}
    if os.path.exists(expfile):
        for line in open(expfile):
            if line.strip() and not line.startswith("#"):
                f = line.split()
                expected[f[0]] = (f[1], int(f[2]), int(f[3]))

    bad = 0
    out = []
    for path in cases:
        name = os.path.basename(path)
        html = open(path, "rb").read()
        pics = []
        rep0 = {}
        for b in args.binaries:
            pic, rep = render(b, html, ua, font, args.width, args.height)
            if pic is None:
                print("   FAILED: %s crashed in %s" % (name, b))
                bad += 1
                pics.append(None)
                continue
            pics.append(pic)
            if not rep0:
                rep0 = rep
        if any(p is None for p in pics):
            continue
        if len(set(hashlib.sha256(p).hexdigest() for p in pics)) != 1:
            print("   FAILED: %s differs between the build stages" % name)
            bad += 1
            continue
        h = hashlib.sha256(pics[0]).hexdigest()
        ink = painted(pics[0])
        glyph = rep0.get("INK", 0)
        out.append((name, h, ink, glyph))
        if args.freeze:
            print("   frozen  %-22s %s  %d octets painted, %d glyph px"
                  % (name, h[:16], ink, glyph))
            continue
        want = expected.get(name)
        if want is None:
            print("   FAILED: %s has no frozen expectation" % name)
            bad += 1
            continue
        wh, wink, wglyph = want
        if wh != h:
            print("   FAILED: %s -- the picture changed (%s != %s)"
                  % (name, h[:16], wh[:16]))
            bad += 1
        if ink * 2 < wink:
            print("   FAILED: %s painted %d octets, %d were frozen -- is "
                  "anything being drawn at all?" % (name, ink, wink))
            bad += 1
        if glyph * 2 < wglyph:
            print("   FAILED: %s set %d glyph pixels, %d were frozen -- the "
                  "letters are missing" % (name, glyph, wglyph))
            bad += 1
        if wh == h and ink * 2 >= wink and glyph * 2 >= wglyph:
            print("   ok      %-22s %d commands, %d octets, %d glyph px"
                  % (name, rep0.get("CMDS", 0), ink, glyph))
    if args.freeze:
        with open(expfile, "w") as f:
            f.write("# tools/paintb3/cases.py --freeze; see the head of that "
                    "file for what the hashes mean\n")
            for n, h, ink, glyph in out:
                f.write("%s %s %d %d\n" % (n, h, ink, glyph))
        print("CASES: %d frozen" % len(out))
        return 0
    print("CASES: %d / %d cases identical in %d build stages"
          % (len(out) - bad, len(cases), len(args.binaries)))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
