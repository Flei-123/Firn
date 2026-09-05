#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/css/gen_encodings.py -- generates lib/css/encoding_data.fi.

The byte stream of a stylesheet is not always UTF-8 (css-syntax-3 3.2), and
the official test file stylesheet_bytes.json checks exactly that: a BOM, an
@charset rule, a protocol encoding and an environment encoding, each of them
in ISO-8859-2, ISO-8859-5, UTF-16LE and UTF-16BE.

The two single-byte encodings need a table of 128 code points each. It is
generated here out of the codecs of Python, so that no number is typed by
hand -- the same way tools/tokenizer/gen_entities.py works for the HTML
character references.

Usage:  python3 tools/css/gen_encodings.py
"""
import os

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TARGET = os.path.join(ROOT, "lib", "css", "encoding_data.fi")

ENCODINGS = [("iso_8859_2", "iso8859_2"), ("iso_8859_5", "iso8859_5")]


def table(codec):
    out = []
    for b in range(0x80, 0x100):
        try:
            out.append(ord(bytes([b]).decode(codec)))
        except UnicodeDecodeError:
            out.append(0xFFFD)
    return out


def main():
    lines = []
    lines.append("// lib/css/encoding_data.fi -- GENERATED, do not edit by hand.")
    lines.append("//")
    lines.append("// Produced by tools/css/gen_encodings.py out of the codecs of")
    lines.append("// Python. One function per single-byte encoding: it maps a byte")
    lines.append("// 0x80..0xFF to its code point; 0x00..0x7F is ASCII and stays as")
    lines.append("// it is (the caller does not ask here at all).")
    lines.append("")
    lines.append("export { iso_8859_2, iso_8859_5 }")
    for name, codec in ENCODINGS:
        t = table(codec)
        lines.append("")
        lines.append("#[no_gc]")
        lines.append("fn %s(b: u32) -> u32 {" % name)
        lines.append("    var t: [u32; 128] = [")
        for i in range(0, 128, 8):
            row = ", ".join("0x%04X as u32" % v for v in t[i:i + 8])
            lines.append("        %s," % row)
        lines.append("    ]")
        lines.append("    if b < 0x80 as u32 {")
        lines.append("        return b")
        lines.append("    }")
        lines.append("    if b > 0xFF as u32 {")
        lines.append("        return 0xFFFD as u32")
        lines.append("    }")
        lines.append("    return t[(b - 0x80 as u32) as usize]")
        lines.append("}")
    lines.append("")
    open(TARGET, "w", encoding="utf-8").write("\n".join(lines))
    print("written: %s (%d lines)" % (TARGET, len(lines)))


main()
