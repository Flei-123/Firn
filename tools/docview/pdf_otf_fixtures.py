#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import io
import os
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pdf_pattern_fixtures import page_pdf, stream

SOURCE = "/usr/share/fonts/opentype/urw-base35/NimbusSans-Regular.otf"
TEXT = "Hello Firn 42"


def subset_font(path, text):
    options = subset.Options()
    options.notdef_outline = True
    options.glyph_names = True
    options.layout_features = []
    font = TTFont(path)
    sub = subset.Subsetter(options)
    sub.populate(text=text)
    sub.subset(font)
    buf = io.BytesIO()
    font.save(buf)
    return buf.getvalue(), font


def raw_cff(otf_bytes):
    return TTFont(io.BytesIO(otf_bytes)).reader["CFF "]


def widths(font, codes):
    hmtx = font["hmtx"]
    cmap = font.getBestCmap()
    upem = font["head"].unitsPerEm
    return [int(hmtx[cmap[c]][0] * 1000 / upem) if c in cmap else 0 for c in codes]


def descriptor(flags=32):
    return "<< /Type /FontDescriptor /FontName /AAAAAA+Nimbus /Flags %d /FontBBox [-200 -300 1200 1000] /ItalicAngle 0 /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 /FontFile3 6 0 R >>" % flags


def make(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    data, font = subset_font(SOURCE, TEXT)
    cff = raw_cff(data)
    cmap = font.getBestCmap()
    order = font.getGlyphOrder()
    gids = [order.index(cmap[ord(c)]) for c in TEXT if ord(c) in cmap]
    hexs = "".join("%04X" % g for g in gids)
    first, last = 32, 126
    wlist = " ".join(str(w) for w in widths(font, range(first, last + 1)))
    cases = {}
    cases["otto_simple"] = page_pdf(
        "BT /F1 40 Tf 10 100 Td (%s) Tj ET" % TEXT,
        "/Font << /F1 5 0 R >>",
        extra=[
            ("<< /Type /Font /Subtype /Type1 /BaseFont /AAAAAA+Nimbus /FirstChar %d /LastChar %d /Widths [%s] /Encoding /WinAnsiEncoding /FontDescriptor 7 0 R >>" % (first, last, wlist)).encode(),
            stream("/Subtype /OpenType", data),
            descriptor().encode(),
        ],
    )
    cases["otto_cid"] = page_pdf(
        "BT /F1 40 Tf 10 100 Td <%s> Tj ET" % hexs,
        "/Font << /F1 5 0 R >>",
        extra=[
            b"<< /Type /Font /Subtype /Type0 /BaseFont /AAAAAA+Nimbus /Encoding /Identity-H /DescendantFonts [8 0 R] >>",
            stream("/Subtype /OpenType", data),
            descriptor().encode(),
            b"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /AAAAAA+Nimbus /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 7 0 R /DW 556 >>",
        ],
    )
    cases["cid0c_raw"] = page_pdf(
        "BT /F1 40 Tf 10 100 Td <%s> Tj ET" % hexs,
        "/Font << /F1 5 0 R >>",
        extra=[
            b"<< /Type /Font /Subtype /Type0 /BaseFont /AAAAAA+Nimbus /Encoding /Identity-H /DescendantFonts [8 0 R] >>",
            stream("/Subtype /CIDFontType0C", cff),
            descriptor().encode(),
            b"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /AAAAAA+Nimbus /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 7 0 R /DW 556 >>",
        ],
    )
    for name, blob in cases.items():
        with open(os.path.join(out_dir, name + ".pdf"), "wb") as handle:
            handle.write(blob)
    return list(cases)


if __name__ == "__main__":
    print(" ".join(make(sys.argv[1])))
