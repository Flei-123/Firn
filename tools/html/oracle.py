#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Counter-check for the HAND-WRITTEN expectations in tools/html/cases/.

WHAT FOR: the `tree-construction` data of html5lib is not available to this
project (see docs/ROUND54.md). The expectations in `tools/html/cases/*.dat`
are therefore written by hand from the WHATWG standard. By hand also means:
error-prone. This script checks them against html5lib 1.1, an
independent, specification-faithful implementation.

IMPORTANT -- WHAT THIS IS AND WHAT IT IS NOT:
  * It is a CHECK of the expectations, not a generation. The expectations
    stand in the .dat files by hand; here it is only reported where they
    differ from html5lib. Every deviation is decided at the standard.
  * html5lib is NOT part of the project and is not shipped with it. It is
    installed in a venv of its own (tools/html/run.sh --check-expectations)
    and used only here.

Usage:  python3 tools/html/oracle.py [files...]
Return: 0 = all expectations agree with html5lib.
"""

import glob
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FAELLE = os.path.join(ROOT, "tools", "html", "cases")
GAPS = os.path.join(ROOT, "tools", "html", "gaps")

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


def load_dat(path):
    """Reads an html5lib .dat file: a list of (data, errors, document)."""
    cases = []
    with open(path, encoding="utf-8") as fh:
        text = fh.read()
    if not text:
        return cases
    for block in text.split("\n#data\n"):
        block = block.lstrip("\n")
        if block.startswith("#data\n"):
            block = block[len("#data\n"):]
        if not block.strip():
            continue
        parts = {}
        cur = "data"
        parts[cur] = []
        for line in block.split("\n"):
            if line.startswith("#") and " " not in line.rstrip():
                cur = line[1:].strip()
                parts[cur] = []
                continue
            parts.setdefault(cur, []).append(line)
        data = "\n".join(parts.get("data", []))
        doc = parts.get("document", [])
        while doc and doc[-1] == "":
            doc.pop()
        context = "\n".join(parts.get("document-fragment", [])).strip() or None
        cases.append((data, "\n".join(doc),
                       "oracle-deviation" in parts, context))
    return cases


def serialisiere(dom):
    zeilen = []

    def attr_name(a):
        if a.namespaceURI in ATTR_NS:
            return ATTR_NS[a.namespaceURI] + a.localName
        return a.name

    def walk(n, depth):
        pre = "| " + "  " * depth
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
                zeilen.append('| %s%s="%s"' % ("  " * (depth + 1), name, wert))
        elif t == n.TEXT_NODE:
            zeilen.append('%s"%s"' % (pre, n.data))
        elif t == n.COMMENT_NODE:
            zeilen.append("%s<!-- %s -->" % (pre, n.data))
        elif t == n.DOCUMENT_TYPE_NODE:
            s = "%s<!DOCTYPE %s" % (pre, n.name or "")
            if n.publicId or n.systemId:
                s += ' "%s" "%s"' % (n.publicId or "", n.systemId or "")
            zeilen.append(s + ">")
        for k in merged(n.childNodes):
            walk(k, depth + 1)

    def merged(children):
        """minidom creates a text node of its own per character token; the
        .dat format knows only ONE per run. Merged here."""
        out = []
        for k in list(children):
            if (out and k.nodeType == k.TEXT_NODE
                    and out[-1].nodeType == k.TEXT_NODE):
                out[-1] = out[-1].cloneNode(False)
                out[-1].data = out[-1].data + k.data
                continue
            out.append(k)
        return out

    for k in merged(dom.childNodes):
        walk(k, 0)
    return "\n".join(zeilen)


def referenz(data, context=None):
    import html5lib
    from html5lib.treebuilders import getTreeBuilder

    p = html5lib.HTMLParser(tree=getTreeBuilder("dom"), namespaceHTMLElements=True)
    if context:
        return serialisiere(p.parseFragment(data, container=context))
    dom = p.parse(data)
    # `dom` is the root element document of minidom
    return serialisiere(dom.ownerDocument or dom)


def main():
    files = sys.argv[1:] or (sorted(glob.glob(os.path.join(FAELLE, "*.dat")))
                               + sorted(glob.glob(os.path.join(GAPS, "*.dat"))))
    ges = 0
    schlecht = 0
    nachlaeufer = 0
    for path in files:
        for i, (data, expected, known, context) in enumerate(load_dat(path)):
            ges += 1
            got = referenz(data, context)
            if got != expected and known:
                nachlaeufer += 1
                continue
            if got != expected:
                schlecht += 1
                print("DEVIATION %s #%d  input=%r" % (os.path.basename(path), i, data))
                print("--- meine Erwartung ---")
                print(expected)
                print("--- html5lib 1.1 ---")
                print(got)
                print()
    print("%d cases checked, %d deviations from html5lib 1.1 "
          "(%d known: html5lib 1.1 follows an older version of the "
          "standard there, noted with '#oracle-deviation')"
          % (ges, schlecht, nachlaeufer))
    return 1 if schlecht else 0


if __name__ == "__main__":
    sys.exit(main())
