#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dialog/fontdlg_fonts.py -- a deterministic font folder for the font dialog tests.

    python3 tools/dialog/fontdlg_fonts.py <outdir> [--generated N]

Writes into <outdir> (which must exist and be empty):

  gen/           N tiny families "Gen Family 000" .. (default 500), every tenth with Bold and Italic faces too
  box/           "Test Box": Regular, Bold, Italic, Bold Italic (typographic names 16/17 AND legacy 1/2)
                 and "Test Box" Light (weight 300) as a style row of its own; glyphs are SQUARES
  bar/           "Test Bar": Regular only, glyphs are thin BARS (so a preview can tell the files apart)
  mac/           "Mac Only": name records of the Macintosh platform only (Roman, with an umlaut: "Mac Äpfel")
  cff/           "Test CFF": an OpenType font with CFF outlines (listed, cannot be drawn by lib/font)
  ttc/           a collection with "Test Collection A" and "Test Collection B"
  broken/        an empty file, a text file, a truncated font, a font without a name table (family = file name)
  dup/, loop/    the "Test Box" Regular a second time (a copy and a symbolic link: shown once) and a link
                 to a directory that points back (must not loop)
  real/          symbolic links to the system DejaVu faces (Sans, Sans Bold, Serif, Sans Mono)

The expected counts are written to <outdir>/EXPECTED as `key=value` lines. Needs fontTools; exit 77 (skip) without it.
"""
import os, shutil, sys

try:
    from fontTools.fontBuilder import FontBuilder
    from fontTools.pens.ttGlyphPen import TTGlyphPen
    from fontTools.ttLib import TTFont, TTCollection
except ImportError:
    print("fontTools missing")
    sys.exit(77)

CHARS = "AaBbYyZz0123 äöüß"


def rect_glyph(x0, y0, x1, y1):
    pen = TTGlyphPen(None)
    pen.moveTo((x0, y0)); pen.lineTo((x0, y1)); pen.lineTo((x1, y1)); pen.lineTo((x1, y0)); pen.closePath()
    return pen.glyph()


def empty_glyph():
    return TTGlyphPen(None).glyph()


def build(path, family, style, weight=400, bold=False, italic=False, shape="box", typo=True, mac_only=False,
          no_name=False, cff=False, upem=1000):
    names = {}
    glyph_order = [".notdef"] + ["g%d" % ord(c) for c in CHARS]
    cmap = {ord(c): "g%d" % ord(c) for c in CHARS}
    fb = FontBuilder(upem, isTTF=not cff)
    fb.setupGlyphOrder(glyph_order)
    fb.setupCharacterMap(cmap)
    glyphs = {}
    for g in glyph_order:
        if g == ".notdef" or g == "g32":
            glyphs[g] = empty_glyph()
        elif shape == "bar":
            glyphs[g] = rect_glyph(100, 0, 160, 700)
        else:
            glyphs[g] = rect_glyph(60, 0, 540, 700)
    if cff:
        from fontTools.pens.t2CharStringPen import T2CharStringPen
        charstrings = {}
        for g in glyph_order:
            pen = T2CharStringPen(600, None)
            if g not in (".notdef", "g32"):
                pen.moveTo((60, 0)); pen.lineTo((60, 700)); pen.lineTo((540, 700)); pen.lineTo((540, 0)); pen.closePath()
            charstrings[g] = pen.getCharString()
        fb.setupCFF("TestCFF-Regular", {"FullName": "Test CFF"}, charstrings, {})
        fb.setupHorizontalMetrics({g: (600, 0) for g in glyph_order})
    else:
        fb.setupGlyf(glyphs)
        fb.setupHorizontalMetrics({g: (600, 60) for g in glyph_order})
    fb.setupHorizontalHeader(ascent=800, descent=-200)
    legacy_style = style
    if style not in ("Regular", "Bold", "Italic", "Bold Italic"):
        legacy_style = "Regular"
    legacy_family = family if legacy_style == style else family + " " + style
    if not no_name:
        nm = {"familyName": legacy_family, "styleName": legacy_style, "uniqueFontIdentifier": family + style,
              "fullName": family + " " + style, "psName": (family + "-" + style).replace(" ", ""), "version": "1.0"}
        if typo and legacy_style != style or typo and weight == 300:
            nm["typographicFamily"] = family
            nm["typographicSubfamily"] = style
        fb.setupNameTable(nm, mac=True)
    fb.setupOS2(usWeightClass=weight, fsSelection=(1 if italic else 0) | (32 if bold else 0) | (64 if not (bold or italic) else 0),
                sTypoAscender=800, sTypoDescender=-200, sTypoLineGap=0, usWinAscent=800, usWinDescent=200)
    fb.setupPost()
    fb.font["head"].macStyle = (1 if bold else 0) | (2 if italic else 0)
    fb.save(path)
    if mac_only:
        f = TTFont(path)
        t = f["name"]
        t.names = [n for n in t.names if n.platformID == 1]
        assert t.names, "no Macintosh names written"
        f.save(path)
    return path


def main():
    out = os.path.abspath(sys.argv[1])
    ngen = 500
    if "--generated" in sys.argv:
        ngen = int(sys.argv[sys.argv.index("--generated") + 1])
    for d in ("gen", "box", "bar", "mac", "cff", "ttc", "broken", "dup", "loop", "real"):
        os.makedirs(os.path.join(out, d), exist_ok=True)
    files = 0
    for i in range(ngen):
        fam = "Gen Family %03d" % i
        build(os.path.join(out, "gen", "gen%03d.ttf" % i), fam, "Regular")
        files += 1
        if i % 10 == 0:
            build(os.path.join(out, "gen", "gen%03d-b.ttf" % i), fam, "Bold", 700, bold=True)
            build(os.path.join(out, "gen", "gen%03d-i.ttf" % i), fam, "Italic", 400, italic=True)
            files += 2
    box = {}
    for style, w, b, it in (("Regular", 400, False, False), ("Bold", 700, True, False), ("Italic", 400, False, True),
                            ("Bold Italic", 700, True, True)):
        box[style] = build(os.path.join(out, "box", "testbox-%s.ttf" % style.replace(" ", "")), "Test Box", style, w, b, it,
                           typo=False)
    build(os.path.join(out, "box", "testbox-light.ttf"), "Test Box", "Light", 300, shape="box", typo=True)
    build(os.path.join(out, "bar", "testbar.ttf"), "Test Bar", "Regular", 400, shape="bar", typo=False)
    build(os.path.join(out, "mac", "mac.ttf"), "Mac Äpfel", "Regular", 400, typo=False, mac_only=True)
    build(os.path.join(out, "cff", "testcff.otf"), "Test CFF", "Regular", 400, cff=True, typo=False)
    a = TTFont(build(os.path.join(out, "ttc", "a.ttf"), "Test Collection A", "Regular", typo=False))
    b2 = TTFont(build(os.path.join(out, "ttc", "b.ttf"), "Test Collection B", "Regular", typo=False))
    coll = TTCollection()
    coll.fonts = [a, b2]
    coll.save(os.path.join(out, "ttc", "pair.ttc"))
    os.remove(os.path.join(out, "ttc", "a.ttf")); os.remove(os.path.join(out, "ttc", "b.ttf"))
    open(os.path.join(out, "broken", "empty.ttf"), "wb").close()
    with open(os.path.join(out, "broken", "text.ttf"), "w") as fh:
        fh.write("this is not a font\n" * 40)
    data = open(box["Regular"], "rb").read()
    with open(os.path.join(out, "broken", "truncated.ttf"), "wb") as fh:
        fh.write(data[:200])
    build(os.path.join(out, "broken", "Nameless-Face.ttf"), "x", "Regular", no_name=True, typo=False)
    shutil.copy(box["Regular"], os.path.join(out, "dup", "testbox-copy.ttf"))
    os.symlink(box["Regular"], os.path.join(out, "dup", "testbox-link.ttf"))
    os.symlink(out, os.path.join(out, "loop", "up"))
    real = "/usr/share/fonts/truetype/dejavu"
    have_real = 0
    for f in ("DejaVuSans.ttf", "DejaVuSans-Bold.ttf", "DejaVuSerif.ttf", "DejaVuSansMono.ttf"):
        if os.path.exists(os.path.join(real, f)):
            os.symlink(os.path.join(real, f), os.path.join(out, "real", f))
            have_real += 1
    fam_total = ngen + 1 + 1 + 1 + 1 + 2 + 1 + (3 if have_real == 4 else 0)   # gen, box, bar, mac, cff, ttc x2, nameless, dejavu x3
    with open(os.path.join(out, "EXPECTED"), "w") as fh:
        fh.write("generated=%d\nfamilies=%d\nreal=%d\n" % (ngen, fam_total, have_real))
    print("fonts written: %d generated families, %d families expected" % (ngen, fam_total))


main()
