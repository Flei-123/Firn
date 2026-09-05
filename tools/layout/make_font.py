#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Builds `tools/layout/FirnMetric.ttf` -- the measuring font of round 61.

A layout engine cannot be compared against a browser as long as the width
of a letter is unknown.  The way out is the one the CSS working group has
been using since 1999: a font whose metrics are a round number.  This
script builds such a font, so that nothing has to be downloaded and the
numbers are visible in the source instead of hidden in a binary blob:

    units per em    1000
    advance width   1000  for EVERY glyph      -> text width = n * font-size
    ascent           800                       -> 0.8 em above the baseline
    descent          200                       -> 0.2 em below the baseline
    line gap           0                       -> `line-height: normal` = 1 em

The glyph outline is a filled rectangle from -200 to 800, except for the
space, which is empty.  The outline does not matter for layout; only the
metrics do.  All three metric sets of an OpenType file (`hhea`, the typo
values of `OS/2` and the win values of `OS/2`) are filled with the SAME
numbers, so that it makes no difference which of them the browser reads.

Requires fontTools (a test tool, not part of the production path).
Usage:  python3 tools/layout/make_font.py [out.ttf]
"""

import os
import sys

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

UPM = 1000
ASCENT = 800
DESCENT = 200
ADVANCE = 1000

# A fixed date, so the file is REPRODUCIBLE (see build()).
FONT_EPOCH = 3553891200

# Every code point that the cases use.  More would not hurt, but this keeps
# the file small and the table readable.
CHARS = [chr(c) for c in range(0x20, 0x7F)]


def box_glyph(inset=0):
    pen = TTGlyphPen(None)
    x0, x1 = inset, ADVANCE - inset
    y0, y1 = -DESCENT, ASCENT
    pen.moveTo((x0, y0))
    pen.lineTo((x1, y0))
    pen.lineTo((x1, y1))
    pen.lineTo((x0, y1))
    pen.closePath()
    return pen.glyph()


def empty_glyph():
    return TTGlyphPen(None).glyph()


def build(path):
    names = [".notdef"]
    cmap = {}
    for ch in CHARS:
        name = "u%04X" % ord(ch)
        names.append(name)
        cmap[ord(ch)] = name

    fb = FontBuilder(UPM, isTTF=True)
    fb.setupGlyphOrder(names)
    fb.setupCharacterMap(cmap)

    glyphs = {".notdef": box_glyph(100)}
    for ch in CHARS:
        name = "u%04X" % ord(ch)
        glyphs[name] = empty_glyph() if ch == " " else box_glyph()
    fb.setupGlyf(glyphs)

    fb.setupHorizontalMetrics(dict((n, (ADVANCE, 0)) for n in names))
    fb.setupHorizontalHeader(ascent=ASCENT, descent=-DESCENT, lineGap=0)
    fb.setupNameTable({
        "familyName": "FirnMetric",
        "styleName": "Regular",
        "fullName": "FirnMetric Regular",
        "psName": "FirnMetric-Regular",
        "version": "Version 1.000",
    })
    fb.setupOS2(
        sTypoAscender=ASCENT, sTypoDescender=-DESCENT, sTypoLineGap=0,
        usWinAscent=ASCENT, usWinDescent=DESCENT,
        sxHeight=ASCENT, sCapHeight=ASCENT,
        achVendID="FIRN", fsType=0, fsSelection=(1 << 6),
    )
    fb.setupPost(isFixedPitch=1, underlinePosition=-DESCENT // 2,
                 underlineThickness=50)
    # ROUND 78: the same input has to give the same file. fontTools stamps
    # `head.created`/`head.modified` with the CURRENT time, so every run
    # produced twelve different octets (the two dates and the checksums
    # over them) and `git status` was dirty after every layout run -- which
    # trains everybody to ignore a dirty tree. The date is pinned instead.
    # TrueType counts seconds since 1904-01-01; 3,553,891,200 is
    # 2016-08-24, the day the font's metrics were fixed and not a second
    # of it matters for layout.
    fb.font["head"].created = FONT_EPOCH
    fb.font["head"].modified = FONT_EPOCH
    fb.save(path)
    return path


if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "FirnMetric.ttf")
    build(out)
    print("wrote %s (upem=%d ascent=%d descent=%d advance=%d)"
          % (out, UPM, ASCENT, DESCENT, ADVANCE))
