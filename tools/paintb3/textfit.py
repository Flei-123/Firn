#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/paintb3/textfit.py -- does the LAYOUT break the line where the
LETTERS end?

Round B2 laid text out with a made-up font: every character exactly one em
wide.  That is right for Ahem and wrong for every real font.  As long as
nothing was drawn, nobody could see it.  Round B3 draws, and the two have
to agree -- otherwise the line breaks in one place and the ink ends in
another, and every box below stands right while the text inside it stands
wrong.

TWO MEASUREMENTS, because the two ways of being wrong look different.

  1. NOTHING STICKS OUT.  A paragraph in a box of known width: no pixel of
     ink may stand right of the content edge.  This catches a layout that
     thinks the text is NARROWER than it is drawn.

  2. THE BOX IS NOT TOO BIG EITHER.  The same text in a
     `display: inline-block`, whose width is shrink-to-fit -- that width
     IS the layout's own opinion of how wide the text is.  Its right edge
     and the right edge of the ink have to be within a few pixels of each
     other.  This catches a layout that thinks the text is WIDER than it
     is drawn, which is what round B2's one-em-per-character font does to
     a proportional typeface, and which measurement 1 cannot see at all.

THE COUNTER-CHECK, and it is what makes measurement 2 mean anything: the
same page is rendered again with flag 4, which leaves the LAYOUT with the
metrics of round B2 while the painter keeps the real font.  The gap has to
get much worse.  If it does not, the check is measuring something that
cannot fail and proves nothing.
"""
import argparse
import os
import struct
import subprocess
import sys

import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

TEXTS = [
    "Wide characters like MMMM and WWWW next to narrow ones like iiii and llll",
    "The quick brown fox jumps over the lazy dog and keeps on running for a while",
    "AVATAR To Ta Yo We LT P. F, kerning pairs that pull letters together",
    "Mixed 0123456789 punctuation: ;,.!? and some accents aeiou nnn ooo",
]
WIDTHS = [120, 200, 320, 500]


def blk(b):
    return struct.pack("<I", len(b)) + b


def read_ppm(data):
    assert data[:2] == b"P6"
    i, vals = 2, []
    while len(vals) < 3:
        while data[i:i + 1].isspace():
            i += 1
        j = i
        while not data[j:j + 1].isspace():
            j += 1
        vals.append(int(data[i:j]))
        i = j
    i += 1
    w, h, _ = vals
    return np.frombuffer(data[i:i + w * h * 3], dtype=np.uint8).reshape(h, w, 3)


def run(binary, html, ua, font, vw, vh, flags):
    j = (struct.pack("<II", vw, vh) + blk(html) + blk(ua) + blk(b"")
         + blk(font) + struct.pack("<I", flags))
    p = subprocess.run([binary], input=j, capture_output=True, timeout=120)
    if p.returncode != 0:
        raise SystemExit("paintb3 failed: %d" % p.returncode)
    return read_ppm(p.stdout)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("binary")
    ap.add_argument("--size", type=int, default=17)
    args = ap.parse_args()
    ua = open(os.path.join(ROOT, "lib/dom/ua.css"), "rb").read()
    font = open(os.path.join(ROOT, "tests/data/fonts/FirnSans.ttf"), "rb").read()

    # ---- 1. nothing sticks out of a fixed width box
    worst_over = 0
    for text in TEXTS:
        for w in WIDTHS:
            html = ("<!doctype html><html><body style='margin:0'>"
                    "<div style='width:%dpx'><p style='margin:0;font-size:%dpx;"
                    "color:#000000'>%s</p></div></body></html>"
                    % (w, args.size, text)).encode()
            a = run(args.binary, html, ua, font, w + 200, 400, 2)
            ink = np.where(a.min(axis=2) < 250)
            if len(ink[1]):
                worst_over = max(worst_over, int(ink[1].max()) + 1 - w)

    # ---- 2. the shrink-to-fit width IS the layout's opinion
    gaps_on, gaps_off = [], []
    for text in TEXTS:
        html = ("<!doctype html><html><body style='margin:0'>"
                "<div style='display:inline-block;background:#eeeeee;"
                "font-size:%dpx;color:#000000'>%s</div></body></html>"
                % (args.size, text)).encode()
        for flags, sink in ((2, gaps_on), (2 | 4, gaps_off)):
            a = run(args.binary, html, ua, font, 1400, 200, flags)
            notwhite = np.where(a.min(axis=2) < 250)
            dark = np.where(a.max(axis=2) < 128)
            if not len(notwhite[1]) or not len(dark[1]):
                sink.append(9999)
                continue
            sink.append(int(notwhite[1].max()) - int(dark[1].max()))

    on = max(gaps_on)
    off = max(gaps_off)
    print("   text fit      %d paragraphs x %d widths at %d px"
          % (len(TEXTS), len(WIDTHS), args.size))
    print("   1. ink outside a fixed width box:            %d px" % worst_over)
    print("   2. shrink-to-fit box wider than its ink:     %d px "
          "(metrics fed back)" % on)
    print("      the same with the round B2 font (flag 4): %d px "
          "(counter-check)" % off)
    if worst_over > 0:
        print("   FAILED: the letters do not fit the boxes the layout made")
        print("TEXTFIT: %d px overflow" % worst_over)
        return 1
    if on > 4:
        print("   FAILED: the layout's own width for the text is %d px wider "
              "than the text it draws" % on)
        print("TEXTFIT: %d px gap" % on)
        return 1
    if off <= on + 8:
        print("   FAILED: the counter-check does not fail -- this measurement "
              "cannot tell the two cases apart and proves nothing")
        print("TEXTFIT: counter-check did not fail")
        return 1
    print("TEXTFIT: OK, 0 px outside the box, %d px gap with the real "
          "metrics against %d px without" % (on, off))
    return 0


if __name__ == "__main__":
    sys.exit(main())
