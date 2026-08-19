#!/usr/bin/env python3
"""Erzeugt lib/browser/tag.fi — die FESTE Namenstabelle des Baumaufbaus.

WERKBANK, KEIN PRODUKT: hier steht keine Parserlogik, nur die Liste der Namen,
die der HTML-Standard beim Namen nennt, und ihre Reihenfolge. Weil die
Reihenfolge fest ist, ist die Atomkennung jedes bekannten Namens eine
Uebersetzungszeitkonstante (`M_DIV`, `M_TABLE`, …) — der Baumaufbau vergleicht
dann `u32` gegen `u32` statt Zeichen gegen Zeichen.

Aufruf:  python3 tools/html/gen_namen.py
"""

import os

# Reihenfolge = Atomkennung (1-basiert). NICHT umsortieren, nur hinten
# anhaengen — sonst aendern sich alle Konstanten.
NAMEN = [
    # --- Grundgeruest
    "html", "head", "body", "frameset", "frame", "noframes", "title",
    # --- "in head"
    "base", "basefont", "bgsound", "link", "meta", "style", "script",
    "noscript", "template",
    # --- Blockelemente ("in body", Gruppe address/div/…)
    "address", "article", "aside", "blockquote", "center", "details",
    "dialog", "dir", "div", "dl", "dt", "dd", "fieldset", "figcaption",
    "figure", "footer", "header", "hgroup", "main", "menu", "nav", "ol",
    "p", "search", "section", "summary", "ul",
    # --- Ueberschriften
    "h1", "h2", "h3", "h4", "h5", "h6",
    # --- Sonderfaelle mit eigener Regel
    "pre", "listing", "form", "li", "plaintext", "button",
    # --- Formatierungselemente (Liste der aktiven Formatierungselemente)
    "a", "b", "big", "code", "em", "font", "i", "nobr", "s", "small",
    "strike", "strong", "tt", "u",
    # --- Elemente mit Marke in der Formatierungsliste
    "applet", "marquee", "object",
    # --- Tabellen
    "table", "caption", "colgroup", "col", "tbody", "tfoot", "thead",
    "tr", "td", "th",
    # --- leere Elemente
    "area", "br", "embed", "img", "keygen", "wbr", "param", "source",
    "track", "hr", "input",
    # --- Text-/Rohtextelemente
    "textarea", "xmp", "iframe", "noembed",
    # --- Auswahl
    "select", "option", "optgroup",
    # --- Ruby
    "rb", "rt", "rtc", "rp",
    # --- Fremdinhalt (nur als Namensraummarker gebraucht, Runde 54)
    "math", "svg", "mi", "mo", "mn", "ms", "mtext", "annotation-xml",
    "foreignObject", "desc",
    # --- vom Baumaufbau umbenannt
    "image",
    # --- Attributnamen, die der Baumaufbau selbst liest
    "type", "id", "class", "href", "src", "name", "action", "prompt",
    # --- nachgetragen (Reihenfolge davor bleibt unveraendert)
    "ruby",
]

KOPF = '''// lib/browser/tag.fi — ERZEUGT von tools/html/gen_namen.py.
// NICHT VON HAND AENDERN. Quelle der Reihenfolge ist das Skript.
//
// Die feste Namenstabelle des Baumaufbaus: {n} Namen, deren Atomkennung
// (1..{n}) zur Uebersetzungszeit feststeht. `namen_init` traegt sie in genau
// dieser Reihenfolge in die Tabelle ein, deshalb gilt `M_DIV == 25` und so
// weiter. Alles darueber hinaus (eigene Elementnamen, beliebige
// Attributnamen) bekommt beim ersten Auftreten eine Kennung > {n}.

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
        "// tests/570_zeichenkettenliterale.fi), deshalb steht die Laenge hier\n"
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
