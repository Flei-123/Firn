#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Produces lib/html/error_codes.fi -- the name table of the WHATWG parse errors.

Stage 0 has no string literals (SPEC 14.1.str S1). The code names are
therefore stored as a sequence of u64 pieces (8 ASCII bytes each,
little-endian, 0 = end) in a generated Firn file.

The list comes from the WHATWG HTML standard 13.2 ("parse errors") and covers
all codes that occur in testdata/html5lib-tokenizer/*.test.

Usage:  python3 tools/tokenizer/gen_errors.py
"""
import os

CODES = [
    "abrupt-closing-of-empty-comment",
    "abrupt-doctype-public-identifier",
    "abrupt-doctype-system-identifier",
    "absence-of-digits-in-numeric-character-reference",
    "cdata-in-html-content",
    "character-reference-outside-unicode-range",
    "control-character-in-input-stream",
    "control-character-reference",
    "duplicate-attribute",
    "end-tag-with-attributes",
    "end-tag-with-trailing-solidus",
    "eof-before-tag-name",
    "eof-in-cdata",
    "eof-in-comment",
    "eof-in-doctype",
    "eof-in-script-html-comment-like-text",
    "eof-in-tag",
    "incorrectly-closed-comment",
    "incorrectly-opened-comment",
    "invalid-character-sequence-after-doctype-name",
    "invalid-first-character-of-tag-name",
    "missing-attribute-value",
    "missing-doctype-name",
    "missing-doctype-public-identifier",
    "missing-doctype-system-identifier",
    "missing-end-tag-name",
    "missing-quote-before-doctype-public-identifier",
    "missing-quote-before-doctype-system-identifier",
    "missing-semicolon-after-character-reference",
    "missing-whitespace-after-doctype-public-keyword",
    "missing-whitespace-after-doctype-system-keyword",
    "missing-whitespace-before-doctype-name",
    "missing-whitespace-between-attributes",
    "missing-whitespace-between-doctype-public-and-system-identifiers",
    "nested-comment",
    "noncharacter-character-reference",
    "noncharacter-in-input-stream",
    "null-character-reference",
    "surrogate-character-reference",
    "surrogate-in-input-stream",
    "unexpected-character-after-doctype-system-identifier",
    "unexpected-character-in-attribute-name",
    "unexpected-character-in-unquoted-attribute-value",
    "unexpected-equals-sign-before-attribute-name",
    "unexpected-null-character",
    "unexpected-question-mark-instead-of-tag-name",
    "unexpected-solidus-in-tag",
    "unknown-named-character-reference",
]

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ZIEL = os.path.join(ROOT, "lib", "html", "error_codes.fi")


def konstante(name):
    return "F_" + name.upper().replace("-", "_")


def stuecke(name):
    roh = name.encode("ascii")
    aus = []
    for i in range(0, len(roh), 8):
        block = roh[i:i + 8]
        v = 0
        for k, b in enumerate(block):
            v |= b << (8 * k)
        aus.append(v)
    return aus


def main():
    max_st = max(len(stuecke(c)) for c in CODES)
    z = []
    z.append("// lib/html/error_codes.fi — Namen der WHATWG-Parse-Fehler (erzeugt).")
    z.append("//")
    z.append("// ERZEUGT von tools/tokenizer/gen_errors.py — nicht von Hand aendern.")
    z.append("//")
    z.append("// Stufe 0 hat keine Zeichenkettenliterale (SPEC §14.1.str S1); jeder")
    z.append("// Codename liegt als Folge von u64-Stuecken vor (8 ASCII-Bytes je Stueck,")
    z.append("// little-endian, ein 0-Byte beendet den Namen).")
    z.append("//")
    z.append("//   name_chunk(code, i) -> u64      Stueck i des Namens von `code`")
    z.append("//   ANZAHL                          Anzahl der Codes (Codes sind 1..ANZAHL)")
    z.append("//   STUECKE_MAX                     hoechste Stueckzahl eines Namens")
    z.append("")
    export = ["ANZAHL", "STUECKE_MAX", "name_chunk"] + [konstante(c) for c in CODES]
    line = "export {"
    for e in export:
        if len(line) + len(e) + 2 > 92:
            z.append(line)
            line = "   "
        line += " " + e + ","
    z.append(line)
    z.append("}")
    z.append("")
    z.append("const ANZAHL: u32 = %d" % len(CODES))
    z.append("const STUECKE_MAX: usize = %d" % max_st)
    z.append("")
    for i, c in enumerate(CODES, start=1):
        z.append("const %s: u32 = %d" % (konstante(c), i))
    z.append("")
    z.append("// Stueck `i` des Namens von `code`; 0 = Ende des Namens.")
    z.append("fn name_chunk(code: u32, i: usize) -> u64 {")
    for i, c in enumerate(CODES, start=1):
        st = stuecke(c)
        z.append("    if code == %d as u32 {" % i)
        for k, v in enumerate(st):
            z.append("        if i == %d { return 0x%X as u64 }" % (k, v))
        z.append("        return 0 as u64")
        z.append("    }")
    z.append("    return 0 as u64")
    z.append("}")
    z.append("")
    with open(ZIEL, "w", encoding="utf-8") as fh:
        fh.write("\n".join(z))
    print("written: %s (%d codes, at most %d pieces)"
          % (os.path.relpath(ZIEL, ROOT), len(CODES), max_st))


if __name__ == "__main__":
    main()
