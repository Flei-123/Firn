#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/xml_cross/gen.py <outdir> <count> <seed> <valid|mutated> [seedfile]
#
# Writes <outdir>/<i>.xml (a document) and <outdir>/<i>.exp (what Python's
# expat / xml.etree says about it) for i < count.
#
#   valid    random well-formed documents: nesting, attributes in both quote
#            styles, entities and character references in every spelling,
#            CDATA, comments, processing instructions, CRLF/CR line ends,
#            non-ASCII text and names, prefixed names, an XML declaration or
#            a BOM now and then.
#   mutated  the same documents damaged at byte level (flips, cuts, inserted
#            syntax fragments ...): the reference decides accepted/refused.
#
# THE REFERENCE is Python's own expat, driven the way xml.etree drives it
# (xml.etree.ElementTree.TreeBuilder builds the tree) but with namespace
# processing OFF, because std.xml keeps `prefix:name` as a plain string while
# `ET.fromstring` would rewrite it to `{uri}name` and refuse unbound prefixes.
# For documents with no colon in any name the script ALSO runs `ET.fromstring`
# and checks that the two references agree (the self check; a disagreement is
# a bug of this script and stops it).
#
# .exp is the dump the Firn side prints (see tools/xml_cross/xml_cli.fi):
#   <name   @key=value   "text   >      with \xHH for octets outside 32..126 and `\`
# or `ERR <line> <col> <message>` when expat refuses the document.
import os, random, sys
import xml.etree.ElementTree as ET
from xml.parsers import expat

out, count, seed, mode = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
seedfile = sys.argv[5] if len(sys.argv) > 5 else None
rng = random.Random(seed)
os.makedirs(out, exist_ok=True)

ASCII_NAMES = ["a", "b", "item", "Node_1", "x-y", "x.y", "_u", "row", "cell", "data",
               "ns:tag", "a:b", "svg:rect", "n1:n2", "xml:lang", "xmlns:z", "root", "e1"]
UNI_NAMES = ["é", "ünï", "日本", "привет", "Ωmega", "a·b", "x́y", "ñ-1", "😀x"[1:]]
UNI_TEXT = ["é", "ü", "ß", "日本語", "Ω", "😀", "€", "я", " ", " ", "﻿", "\U0001f600"]
SAFE = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .,;:!?-_()[]{}#*+/=@$^~|"


def rand_name(first_of=None):
    r = rng.random()
    if r < 0.55:
        return rng.choice(ASCII_NAMES)
    if r < 0.75:
        return rng.choice(UNI_NAMES + ["é1", "a日", "_é"])
    n = rng.randint(1, 8)
    s = rng.choice("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_")
    s += "".join(rng.choice("abcdefghijklmnopqrstuvwxyz0123456789_-.") for _ in range(n - 1))
    return s


def rand_text_piece():
    # a list of (kind, text) with kind "lit" (raw char) or "esc" (already escaped)
    pieces = []
    for _ in range(rng.randint(1, 8)):
        r = rng.random()
        if r < 0.45:
            pieces.append(("lit", "".join(rng.choice(SAFE) for _ in range(rng.randint(1, 12)))))
        elif r < 0.55:
            pieces.append(("lit", rng.choice(UNI_TEXT)))
        elif r < 0.62:
            pieces.append(("esc", rng.choice(["&amp;", "&#38;", "&#x26;"])))
        elif r < 0.68:
            pieces.append(("esc", rng.choice(["&lt;", "&#60;", "&#x3C;", "&#x3c;"])))
        elif r < 0.73:
            pieces.append(("gt", ""))
        elif r < 0.77:
            pieces.append(("esc", rng.choice(["&quot;", "&apos;", "&#34;", "&#39;"])))
        elif r < 0.81:
            cp = rng.choice([65, 0xE9, 0x20AC, 0x1F600, 0x10FFFF, 0xD7FF, 0xE000, 0xFFFD, 0x7F, 0x85, 0x2028])
            pieces.append(("esc", rng.choice(["&#%d;" % cp, "&#x%X;" % cp, "&#x%x;" % cp])))
        elif r < 0.87:
            pieces.append(("lit", rng.choice(["\n", "\n", "\r\n", "\r", "\t", "  ", "\n  "])))
        elif r < 0.9:
            pieces.append(("esc", rng.choice(["&#13;", "&#10;", "&#9;", "&#xD;"])))
        elif r < 0.95:
            body = "".join(rng.choice(SAFE + "<>&\n\r\té") for _ in range(rng.randint(0, 10)))
            if rng.random() < 0.3:
                body += "]]" + ">"  # forces a split below
            pieces.append(("cdata", body))
        elif r < 0.97:
            pieces.append(("esc", "<!--" + "".join(rng.choice("ab c<>&") for _ in range(rng.randint(0, 6))).rstrip("-") + "-->"))
        else:
            pieces.append(("esc", "<?pi%d %s?>" % (rng.randint(0, 9), "".join(rng.choice("ab c<>&") for _ in range(rng.randint(0, 5))))))
    s = ""
    for kind, t in pieces:
        if kind == "lit":
            s += t
        elif kind == "esc":
            s += t
        elif kind == "gt":
            s += ">" if not s.endswith("]]") else "&gt;"
        else:
            parts = t.split("]]>")
            for k, p in enumerate(parts):
                if k:
                    s += "<![CDATA[]]]]><![CDATA[>]]>"
                s += "<![CDATA[" + p + "]]>"
    return s


def rand_attr_value(q):
    s = ""
    for _ in range(rng.randint(0, 6)):
        r = rng.random()
        if r < 0.5:
            s += "".join(rng.choice(SAFE) for _ in range(rng.randint(1, 8)))
        elif r < 0.58:
            s += rng.choice(UNI_TEXT)
        elif r < 0.64:
            s += rng.choice(["&amp;", "&lt;", "&gt;", "&#38;", "&#x3C;"])
        elif r < 0.70:
            s += "&quot;" if q == '"' else "&apos;"
        elif r < 0.74:
            s += rng.choice(['"', "'"]).replace(q, "")  # the other quote, raw
        elif r < 0.80:
            s += rng.choice(["\t", "\n", "\r\n", "\r"])  # normalised to blanks
        elif r < 0.86:
            s += rng.choice(["&#9;", "&#10;", "&#13;", "&#x20;", "&#xA;"])
        elif r < 0.9:
            s += ">"
        else:
            s += "&#x%X;" % rng.choice([0xE9, 0x20AC, 0x1F600])
    return s


def rand_misc():
    r = rng.random()
    if r < 0.3:
        return "<!--" + "".join(rng.choice("abc <>&\n") for _ in range(rng.randint(0, 8))).rstrip("-") + "-->"
    if r < 0.45:
        return "<?proc%d data here?>" % rng.randint(0, 9)
    if r < 0.6:
        return "<?x?>"
    return rng.choice([" ", "\n", "\r\n", "\t", ""])


def rand_element(depth, max_depth, budget):
    name = rand_name()
    s = "<" + name
    used = set()
    for _ in range(rng.choice([0, 0, 1, 1, 2, 3, 5])):
        an = rand_name()
        if an in used or an == name and False:
            continue
        used.add(an)
        q = rng.choice(['"', "'"])
        s += rng.choice([" ", " ", "  ", "\n ", "\t"]) + an + rng.choice(["=", " = ", "=\n"]) + q + rand_attr_value(q) + q
    s += rng.choice(["", "", " ", "\n"])
    r = rng.random()
    kids = 0 if (depth >= max_depth or budget[0] <= 0) else rng.choice([0, 0, 1, 2, 3, 4])
    if r < 0.2 and kids == 0:
        return s + "/>"
    s += ">"
    for _ in range(max(kids, 1) if rng.random() < 0.8 else 0):
        if rng.random() < 0.55:
            s += rand_text_piece()
        if kids and budget[0] > 0:
            budget[0] -= 1
            s += rand_element(depth + 1, max_depth, budget)
    if rng.random() < 0.6:
        s += rand_text_piece()
    return s + "</" + name + rng.choice(["", "", " ", "\n"]) + ">"


def rand_doc():
    max_depth = rng.choice([2, 3, 4, 6, 8, 12, 40])
    body = rand_element(0, max_depth, [rng.choice([5, 15, 40, 120])])
    s = ""
    if rng.random() < 0.4:
        s += '<?xml version="%s"' % rng.choice(["1.0", "1.0", "1.1"])
        if rng.random() < 0.7:
            s += " encoding=%s%s%s" % ((q := rng.choice(['"', "'"])), rng.choice(["UTF-8", "utf-8", "Utf-8"]), q)
        if rng.random() < 0.3:
            s += ' standalone="%s"' % rng.choice(["yes", "no"])
        s += rng.choice(["?>", " ?>"])
        s += rng.choice(["", "\n", "\r\n"])
    for _ in range(rng.choice([0, 0, 1, 2])):
        s += rand_misc()
    s += body
    for _ in range(rng.choice([0, 0, 1, 2])):
        s += rand_misc()
    b = s.encode("utf-8")
    if rng.random() < 0.05:
        b = b"\xef\xbb\xbf" + b
    return b


FRAGS = [b"<", b">", b"</", b"/>", b"&", b"&amp;", b"&#", b"&#x41;", b"&#0;", b'"', b"'", b"=",
         b"<!--", b"-->", b"--", b"<![CDATA[", b"]]>", b"<?", b"?>", b" ", b"\n", b":", "é".encode(),
         b"\xff", b"\x00", b"\x01", b"<!DOCTYPE a>", b"&#xD800;", b"&#x110000;", b"xmlns:", b"\r\n"]


def mutate(b):
    b = bytearray(b)
    for _ in range(rng.choice([1, 1, 1, 2, 3])):
        k = rng.randrange(8)
        if not b:
            b += b"<"
            continue
        i = rng.randrange(len(b))
        if k == 0:
            b[i] ^= 1 << rng.randrange(8)
        elif k == 1:
            b[i] = rng.randrange(256)
        elif k == 2:
            del b[i:i + rng.randint(1, 8)]
        elif k in (3, 4):
            b[i:i] = rng.choice(FRAGS)
        elif k == 5:
            del b[i:]
        elif k == 6:
            b[i] = rng.choice(FRAGS)[0]
        else:
            b[i:i] = b[i:i + rng.randint(1, 16)]
    return bytes(b)


def esc(s):
    r = []
    for c in s.encode("utf-8"):
        if 32 <= c < 127 and c != 92:
            r.append(chr(c))
        else:
            r.append("\\x%02x" % c)
    return "".join(r)


def dump(root):
    lines = []
    stack = [("open", root)]
    while stack:
        kind, e = stack.pop()
        if kind == "close":
            lines.append(">")
            continue
        if kind == "tail":
            lines.append('"' + esc(e))
            continue
        lines.append("<" + esc(e.tag))
        for k, v in e.attrib.items():
            lines.append("@" + esc(k) + "=" + esc(v))
        if e.text:
            lines.append('"' + esc(e.text))
        stack.append(("close", e))
        # push children in reverse so that they pop in order, each followed by its tail
        items = []
        for c in e:
            items.append(("open", c))
            if c.tail:
                items.append(("tail", c.tail))
        for it in reversed(items):
            stack.append(it)
    return "\n".join(lines) + "\n"


def reference(b):
    """Return ('ok', dump) or ('err', line, col, msg)."""
    p = expat.ParserCreate()  # no namespace processing
    tb = ET.TreeBuilder()
    p.buffer_text = True
    p.StartElementHandler = lambda name, attrs: tb.start(name, attrs)
    p.EndElementHandler = lambda name: tb.end(name)
    p.CharacterDataHandler = lambda data: tb.data(data)
    try:
        p.Parse(b, True)
        root = tb.close()
    except expat.ExpatError as e:
        return ("err", e.lineno, e.offset, str(e).split(":")[0])
    except (LookupError, ValueError) as e:
        # an `encoding=` that Python has no codec for
        return ("err", 1, 0, "unknown encoding")
    d = dump(root)
    if b":" not in b:
        # the self check: ET.fromstring (namespace aware) must agree where it can
        try:
            d2 = dump(ET.fromstring(b))
        except Exception as e:  # pragma: no cover
            raise SystemExit("self check: ET.fromstring refused what expat accepted: %r" % (e,))
        if d != d2:
            raise SystemExit("self check: the two references disagree")
    return ("ok", d)


seeds = []
n_ok = n_err = 0
for i in range(count):
    b = rand_doc()
    if mode == "mutated":
        b = mutate(b)
    open(os.path.join(out, "%d.xml" % i), "wb").write(b)
    r = reference(b)
    if r[0] == "ok":
        n_ok += 1
        exp = r[1]
    else:
        n_err += 1
        exp = "ERR %d %d %s\n" % (r[1], r[2], r[3])
    open(os.path.join(out, "%d.exp" % i), "w", newline="").write(exp)
    if seedfile and len(seeds) < 300 and mode == "valid" and r[0] == "ok" and b"\n%%" not in b and b"%%" not in b[-3:]:
        seeds.append(b)
if seedfile:
    with open(seedfile, "wb") as f:
        f.write(b"\n%%\n".join(seeds))
print("gen %s: %d documents, reference accepts %d, refuses %d" % (mode, count, n_ok, n_err))
