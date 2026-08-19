#!/usr/bin/env python3
"""Gegenprobe fuer die HANDGESCHRIEBENEN Erwartungen in tools/html/cases/.

WOZU: die `tree-construction`-Daten von html5lib liegen diesem Projekt nicht
vor (siehe docs/RUNDE54.md). Die Erwartungen in `tools/html/cases/*.dat`
sind deshalb von Hand aus dem WHATWG-Standard geschrieben. Von Hand heisst
auch: fehleranfaellig. Dieses Skript prueft sie gegen html5lib 1.1, eine
unabhaengige, spezifikationstreue Umsetzung.

WICHTIG — WAS DAS IST UND WAS NICHT:
  * Es ist eine PRUEFUNG der Erwartungen, keine Erzeugung. Die Erwartungen
    stehen von Hand in den .dat-Dateien; hier wird nur gemeldet, wo sie von
    html5lib abweichen. Jede Abweichung wird am Standard entschieden.
  * html5lib ist NICHT Teil des Projekts und wird nicht mitgeliefert. Es wird
    in einer eigenen venv installiert (tools/html/run.sh --pruefe-erwartungen)
    und nur hier benutzt.

Aufruf:  python3 tools/html/orakel.py [dateien…]
Rueckgabe: 0 = alle Erwartungen stimmen mit html5lib ueberein.
"""

import glob
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FAELLE = os.path.join(ROOT, "tools", "html", "faelle")
LUECKEN = os.path.join(ROOT, "tools", "html", "luecken")

NS = {
    "http://www.w3.org/1999/xhtml": "",
    "http://www.w3.org/2000/svg": "svg ",
    "http://www.w3.org/1998/Math/MathML": "math ",
}
ATTR_NS = {
    "http://www.w3.org/1999/xlink": "xlink ",
    "http://www.w3.org/XML/1998/namespace": "xml ",
    "http://www.w3.org/2000/xmlns/": "xmlns ",
}


def lade_dat(pfad):
    """Liest eine html5lib-.dat-Datei: Liste von (data, errors, document)."""
    faelle = []
    with open(pfad, encoding="utf-8") as fh:
        text = fh.read()
    if not text:
        return faelle
    for block in text.split("\n#data\n"):
        block = block.lstrip("\n")
        if block.startswith("#data\n"):
            block = block[len("#data\n"):]
        if not block.strip():
            continue
        teile = {}
        aktuell = "data"
        teile[aktuell] = []
        for zeile in block.split("\n"):
            if zeile.startswith("#") and " " not in zeile.rstrip():
                aktuell = zeile[1:].strip()
                teile[aktuell] = []
                continue
            teile.setdefault(aktuell, []).append(zeile)
        data = "\n".join(teile.get("data", []))
        doc = teile.get("document", [])
        while doc and doc[-1] == "":
            doc.pop()
        kontext = "\n".join(teile.get("document-fragment", [])).strip() or None
        faelle.append((data, "\n".join(doc),
                       "orakel-abweichung" in teile, kontext))
    return faelle


def serialisiere(dom):
    zeilen = []

    def attr_name(a):
        if a.namespaceURI in ATTR_NS:
            return ATTR_NS[a.namespaceURI] + a.localName
        return a.name

    def gehe(n, tiefe):
        pre = "| " + "  " * tiefe
        t = n.nodeType
        if t == n.ELEMENT_NODE:
            praefix = NS.get(n.namespaceURI, "")
            zeilen.append("%s<%s%s>" % (pre, praefix, n.localName or n.tagName))
            attrs = []
            if n.attributes:
                for i in range(n.attributes.length):
                    a = n.attributes.item(i)
                    attrs.append((attr_name(a), a.value))
            for name, wert in sorted(attrs):
                zeilen.append('| %s%s="%s"' % ("  " * (tiefe + 1), name, wert))
        elif t == n.TEXT_NODE:
            zeilen.append('%s"%s"' % (pre, n.data))
        elif t == n.COMMENT_NODE:
            zeilen.append("%s<!-- %s -->" % (pre, n.data))
        elif t == n.DOCUMENT_TYPE_NODE:
            s = "%s<!DOCTYPE %s" % (pre, n.name or "")
            if n.publicId or n.systemId:
                s += ' "%s" "%s"' % (n.publicId or "", n.systemId or "")
            zeilen.append(s + ">")
        for k in verschmolzen(n.childNodes):
            gehe(k, tiefe + 1)

    def verschmolzen(kinder):
        """minidom legt je Zeichentoken einen eigenen Textknoten an; das
        .dat-Format kennt nur EINEN je Folge. Hier zusammengefasst."""
        raus = []
        for k in list(kinder):
            if (raus and k.nodeType == k.TEXT_NODE
                    and raus[-1].nodeType == k.TEXT_NODE):
                raus[-1] = raus[-1].cloneNode(False)
                raus[-1].data = raus[-1].data + k.data
                continue
            raus.append(k)
        return raus

    for k in verschmolzen(dom.childNodes):
        gehe(k, 0)
    return "\n".join(zeilen)


def referenz(data, kontext=None):
    import html5lib
    from html5lib.treebuilders import getTreeBuilder

    p = html5lib.HTMLParser(tree=getTreeBuilder("dom"), namespaceHTMLElements=True)
    if kontext:
        return serialisiere(p.parseFragment(data, container=kontext))
    dom = p.parse(data)
    # `dom` ist das Wurzelelement-Dokument von minidom
    return serialisiere(dom.ownerDocument or dom)


def main():
    dateien = sys.argv[1:] or (sorted(glob.glob(os.path.join(FAELLE, "*.dat")))
                               + sorted(glob.glob(os.path.join(LUECKEN, "*.dat"))))
    ges = 0
    schlecht = 0
    nachlaeufer = 0
    for pfad in dateien:
        for i, (data, erwartet, bekannt, kontext) in enumerate(lade_dat(pfad)):
            ges += 1
            ist = referenz(data, kontext)
            if ist != erwartet and bekannt:
                nachlaeufer += 1
                continue
            if ist != erwartet:
                schlecht += 1
                print("ABWEICHUNG %s #%d  input=%r" % (os.path.basename(pfad), i, data))
                print("--- meine Erwartung ---")
                print(erwartet)
                print("--- html5lib 1.1 ---")
                print(ist)
                print()
    print("%d Faelle geprueft, %d Abweichungen zu html5lib 1.1 "
          "(%d bekannte: html5lib 1.1 folgt dort einer aelteren Fassung "
          "des Standards, mit '#orakel-abweichung' vermerkt)"
          % (ges, schlecht, nachlaeufer))
    return 1 if schlecht else 0


if __name__ == "__main__":
    sys.exit(main())
