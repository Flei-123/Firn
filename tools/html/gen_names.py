#!/usr/bin/env python3
"""Erzeugt lib/browser/tag.fi — die FESTE Namenstabelle des Baumaufbaus.

WERKBANK, KEIN PRODUKT: hier steht keine Parserlogik, nur die Liste der Namen,
die der HTML-Standard beim Namen nennt, und ihre Reihenfolge. Weil die
Reihenfolge fest ist, ist die Atomkennung jedes bekannten Namens eine
Uebersetzungszeitkonstante (`M_DIV`, `M_TABLE`, …) — der Baumaufbau vergleicht
dann `u32` gegen `u32` statt Zeichen gegen Zeichen.

Aufruf:  python3 tools/html/gen_names.py
"""

import os

# The order = the atom id (1-based). Do NOT re-sort, only append at the
# end -- otherwise all the constants change.
NAMEN = [
    # --- basic skeleton
    "html", "head", "body", "frameset", "frame", "noframes", "title",
    # --- "in head"
    "base", "basefont", "bgsound", "link", "meta", "style", "script",
    "noscript", "template",
    # --- block elements ("in body", group address/div/...)
    "address", "article", "aside", "blockquote", "center", "details",
    "dialog", "dir", "div", "dl", "dt", "dd", "fieldset", "figcaption",
    "figure", "footer", "header", "hgroup", "main", "menu", "nav", "ol",
    "p", "search", "section", "summary", "ul",
    # --- headings
    "h1", "h2", "h3", "h4", "h5", "h6",
    # --- special cases with a rule of their own
    "pre", "listing", "form", "li", "plaintext", "button",
    # --- formatting elements (list of active formatting elements)
    "a", "b", "big", "code", "em", "font", "i", "nobr", "s", "small",
    "strike", "strong", "tt", "u",
    # --- elements with a marker in the formatting list
    "applet", "marquee", "object",
    # --- tables
    "table", "caption", "colgroup", "col", "tbody", "tfoot", "thead",
    "tr", "td", "th",
    # --- empty elements
    "area", "br", "embed", "img", "keygen", "wbr", "param", "source",
    "track", "hr", "input",
    # --- text/raw text elements
    "textarea", "xmp", "iframe", "noembed",
    # --- select
    "select", "option", "optgroup",
    # --- ruby
    "rb", "rt", "rtc", "rp",
    # --- foreign content (only needed as a namespace marker, round 54)
    "math", "svg", "mi", "mo", "mn", "ms", "mtext", "annotation-xml",
    "foreignObject", "desc",
    # --- renamed by the tree building
    "image",
    # --- attribute names the tree building reads itself
    "type", "id", "class", "href", "src", "name", "action", "prompt",
    # --- added later (the order before that stays unchanged)
    "ruby",
]

KOPF = '''// lib/browser/tag.fi — ERZEUGT von tools/html/gen_names.py.
// DO NOT EDIT BY HAND. The source of the order is the script.
//
// The fixed name table of the tree building: {n} names whose atom id
// (1..{n}) is settled at compile time. `namen_init` enters them in exactly
// this order into the table, which is why `M_DIV == 25` and so
// on holds. Everything beyond that (own element names, arbitrary
// attribute names) gets an id > {n} at its first appearance.

import html.mem

export {{ M_LETZTES_FEST, tab_bytes, {consts} }}

'''


def main():
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    ziel = os.path.join(root, "lib", "browser", "tag.fi")

    if len(set(NAMEN)) != len(NAMEN):
        doppelt = [x for x in NAMEN if NAMEN.count(x) > 1]
        raise SystemExit("doppelte namen: %s" % sorted(set(doppelt)))

    def konst(name):
        return "M_" + name.upper().replace("-", "_")

    consts = ", ".join(konst(x) for x in NAMEN)
    text = " ".join(NAMEN)
    roh = text.encode("ascii")

    out = [KOPF.format(n=len(NAMEN), consts=consts)]
    out.append("const M_LETZTES_FEST: u32 = %d\n\n" % len(NAMEN))
    for i, name in enumerate(NAMEN, start=1):
        out.append("const %s: u32 = %d\n" % (konst(name), i))
    out.append("\n")
    out.append(
        "// Die Namen, durch ein Leerzeichen getrennt. `namen_init` zerlegt sie.\n"
        "// Ein Zeichenkettenliteral hat in Firn den Typ `[u8; N]` (SPEC §8,\n"
        "// tests/570_string_literals.fi), deshalb steht die Laenge hier\n"
        "// ausgeschrieben — das Skript rechnet sie aus.\n"
        "#[no_gc]\n"
        "fn tab_bytes(out: *mut mem.Buf) {\n"
    )
    out.append('    var t: [u8; %d] = "%s"\n' % (len(roh), text))
    out.append("    var i: usize = 0\n")
    out.append("    while i < %d {\n" % len(roh))
    out.append("        mem.buf_push(out, t[i])\n")
    out.append("        i = i + 1\n")
    out.append("    }\n")
    out.append("}\n")

    with open(ziel, "w", encoding="utf-8") as fh:
        fh.write("".join(out))
    print("geschrieben: %s (%d Namen, %d Bytes Text)" % (ziel, len(NAMEN), len(roh)))


if __name__ == "__main__":
    main()
