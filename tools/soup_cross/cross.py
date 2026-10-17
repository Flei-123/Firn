#!/usr/bin/env python3
"""Cross check of lib/html/soup.fi against bs4 (html5lib parser) + soupsieve.

  cross.py gen   N SEED IN.bin     random (also broken) pages + selectors
  cross.py check IN.bin OUT.txt    compare the Firn answers with bs4

Both sides run the HTML5 tree builder and a CSS selector engine; bs4's html5lib
is the reference implementation of the same WHATWG algorithm.

Known reference quirks left out on purpose (soupsieve differs from browsers/Selectors 4):
`:nth-child(An+B)` with A>0 on some trees (soupsieve misses the 2nd li of `n+2`), `:nth-child(odd)` on an only child, and `:empty` with whitespace-only text (tests/3750 covers them by hand).
"""
import random
import sys

TAGS = ["div", "span", "p", "a", "b", "i", "section", "h1", "h2", "em"]
FORMAT = ("a", "b", "i", "em")
INLINE = ["span", "b", "i", "em"]
CLASSES = ["item", "sale", "x", "y", "list", "price", "nav", "a1"]
IDS = ["one", "two", "main", "nav"]
WORDS = ["Red", "Desk", "Fan", "Mug", "x & y", "5 < 6", "caf\u00e9", "\u00fcber", "&amp;", "&lt;b&gt;", "&copy;", "&#65;", " ", "  ", "\n", "t"]
HREFS = ["/p/1", "https://example.com/a", "mailto:x@y.z", "#top", "a b"]
STRAY = False
SELS = [
    "a", "div", "p", "li", "span", "*", ".item", ".sale", ".item.sale", "#one", "#main", "div.item", "a[href]",
    "a[href^=http]", "a[href$='1']", "a[href*=example]", "a:not([href])", "[class~=x]", "[id]", "div > a",
    "div a", "div > div", "ul > li", "li:first-child", "li:last-child", "li:nth-child(2)", "li:nth-child(even)",
    "p:only-child", "p + p", "p ~ span", "span + a", ".x > .y", "div:not(.item)", "div > :not(a)", "ul li a",
    "li:nth-of-type(2)", "td:first-child", "tr > td", "div, span", "a, .item", "h1 ~ h2", ".item a[href]",
    "body > div", "html > body", "b i", "em:only-of-type", ":root", "div:first-of-type", "p:last-of-type",
    "a[href=\"/p/1\"]", "[class^=it]", "[class|=x]", "li:nth-child(3)", "li:nth-last-child(1)",
]


def esc_text(r):
    return r.replace("&", "&amp;").replace("<", "&lt;")


def gen_node(r, depth, parts, inline=False):
    kind = r.random()
    if depth > 4 or kind < 0.25:
        parts.append(r.choice(WORDS))
        return
    if kind < 0.30:
        parts.append("<!-- " + r.choice(WORDS).replace("--", "") + " -->")
        return
    if kind < 0.33 and not inline:
        parts.append("<ul>" if r.random() < 0.8 else '<ul class="nav">')
        for _ in range(r.randint(1, 4)):
            parts.append("<li>")
            gen_node(r, depth + 2, parts)
            if r.random() < 0.5:
                parts.append("</li>")
        if r.random() < 0.75:
            parts.append("</ul>")
        return
    if kind < 0.36 and not inline:
        # tables only well-formed: html5lib's etree tree builder loses foster-parented nodes
        parts.append("<table>")
        for _ in range(r.randint(1, 2)):
            parts.append("<tr>")
            for _ in range(r.randint(1, 3)):
                parts.append("<td>")
                gen_node(r, depth + 2, parts)
                parts.append("</td>")
            parts.append("</tr>")
        parts.append("</table>")
        return
    tag = r.choice(INLINE if inline else TAGS)
    attrs = ""
    if r.random() < 0.5:
        attrs += ' class="%s"' % " ".join(r.sample(CLASSES, r.randint(1, 2)))
    if r.random() < 0.25:
        attrs += " id=%s" % r.choice(IDS)
    if tag == "a" and r.random() < 0.8:
        attrs += ' href="%s"' % r.choice(HREFS)
    if r.random() < 0.05:
        attrs += " data-x"
    parts.append("<%s%s>" % (tag, attrs))
    # formatting elements hold inline content only: no block inside means no adoption agency
    # algorithm, where html5lib 1.1 and the current WHATWG text part ways
    for _ in range(r.randint(0, 4)):
        gen_node(r, depth + 1, parts, inline or tag in FORMAT)
    # broken markup on purpose: sometimes no end tag, sometimes a stray one
    if r.random() < 0.75:
        parts.append("</%s>" % tag)
    elif STRAY and r.random() < 0.3:
        parts.append("</%s>" % r.choice(TAGS))


def gen_page(r):
    parts = []
    if r.random() < 0.5:
        parts.append("<!DOCTYPE html><html><head><title>%s</title></head><body>" % r.choice(WORDS))
    for _ in range(r.randint(1, 6)):
        gen_node(r, 0, parts)
    if r.random() < 0.5:
        parts.append("</body></html>")
    return "".join(parts)


def rec(kind, text):
    b = text.encode("utf-8")
    return b"%s %d\n" % (kind.encode(), len(b)) + b + b"\n"


def gen(n, seed, path):
    r = random.Random(seed)
    out = []
    for _ in range(n):
        page = gen_page(r)
        out.append(rec("D", page))
        for s in r.sample(SELS, 8):
            out.append(rec("S", s))
    open(path, "wb").write(b"".join(out))


def gen_fuzz(n, seed, path):
    """Garbage pages (random bytes, mutated markup) and random selector text: only 'no crash' counts."""
    r = random.Random(seed)
    out = []
    frag = ["<", ">", "</", "<!--", "-->", "&", "&#", "&#x", "=", '"', "'", "<div", "<a href=", "<table>", "<td>",
            "<svg>", "<math>", "<template>", "<select>", "<p>", "</p>", "<b>", "<script>", "</script>", "\x00", "\ufffd"]
    sel_frag = [".", "#", "[", "]", ">", "+", "~", ",", ":", "(", ")", "not", "nth-child", "a", "div", "*", "=", '"',
                "n", "2", "-", " ", "\\", "^=", "$=", "i", ":first-child", ":empty", ":nth-of-type(2)"]
    for _ in range(n):
        if r.random() < 0.3:
            page = bytes(r.randrange(256) for _ in range(r.randint(0, 300)))
        else:
            base = gen_page(r)
            parts = list(base)
            for _ in range(r.randint(0, 6)):
                parts.insert(r.randrange(len(parts) + 1), r.choice(frag))
            page = "".join(parts).encode("utf-8")
        out.append(b"D %d\n" % len(page) + page + b"\n")
        for _ in range(6):
            sel = "".join(r.choice(sel_frag) for _ in range(r.randint(0, 8))).encode()
            out.append(b"S %d\n" % len(sel) + sel + b"\n")
    open(path, "wb").write(b"".join(out))


def esc(s):
    return s.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r")


def read_records(path):
    data = open(path, "rb").read()
    i = 0
    recs = []
    while i < len(data):
        j = data.index(b"\n", i)
        kind, n = data[i:j].split()
        n = int(n)
        recs.append((kind.decode(), data[j + 1:j + 1 + n].decode("utf-8")))
        i = j + 1 + n + 1
    return recs


def text_of(e):
    # own walk over .contents: bs4's get_text() can crash on trees html5lib reparented
    from bs4 import Comment, NavigableString
    out = []
    for c in e.contents:
        if isinstance(c, Comment):
            continue
        if isinstance(c, NavigableString):
            out.append(str(c))
        else:
            out.append(text_of(c))
    return "".join(out)


def to_bs4(text):
    import html5lib
    from bs4 import BeautifulSoup, Comment, NavigableString
    root = html5lib.parse(text, treebuilder="etree", namespaceHTMLElements=False)
    soup = BeautifulSoup("", "html.parser")

    def conv(el):
        t = soup.new_tag(el.tag, attrs=dict(el.attrib))
        if el.text:
            t.append(NavigableString(el.text))
        for c in el:
            if callable(c.tag):
                t.append(Comment(c.text or ""))
            else:
                t.append(conv(c))
            if c.tail:
                t.append(NavigableString(c.tail))
        return t

    soup.append(conv(root))
    return soup


def blocks(lines):
    out = []
    for l in lines:
        if l.startswith("R ") and (not out or len(out[-1]) - 1 == int(out[-1][0][2:])):
            out.append([l])
        elif out:
            out[-1].append(l)
    return out


def expected(path):
    from bs4 import BeautifulSoup
    out = []
    meta = []
    soup = None
    page = ""
    for kind, text in read_records(path):
        if kind == "D":
            # bs4's own html5lib tree builder mis-reparents some broken pages (lost nodes), so
            # the tree comes from html5lib directly and is copied into bs4 only for soupsieve.
            soup = to_bs4(text)
            page = text
            continue
        hits = soup.select(text)
        blk = ["R %d" % len(hits)]
        for e in hits:
            blk.append("\t".join([esc(e.name), esc(e.get("id", "")), esc(e.get("href", "")),
                                  esc(text_of(e))]))
        out.append(blk)
        meta.append((page, text))
    return out, meta


def check(inp, outp):
    want, meta = expected(inp)
    got = open(outp, "rb").read().decode("utf-8", "replace").split("\n")
    if got and got[-1] == "":
        got.pop()
    # re-split the answers into blocks using the counts
    gb = []
    i = 0
    while i < len(got) and got[i].startswith("R "):
        n = int(got[i][2:])
        gb.append(got[i:i + 1 + n])
        i += 1 + n
    bad = 0
    for k in range(len(want)):
        g = gb[k] if k < len(gb) else ["<missing>"]
        if want[k] != g:
            bad += 1
            if bad <= 4:
                print("  MISMATCH selector %r\n    page %r\n    want %r\n    got  %r" % (
                    meta[k][1], meta[k][0][:300], want[k][:4], g[:4]))
    print("soup_cross: %d selector runs, %d mismatches" % (len(want), bad))
    return 0 if bad == 0 and len(gb) == len(want) else 1


if __name__ == "__main__":
    if sys.argv[1] == "fuzz":
        gen_fuzz(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4])
    elif sys.argv[1] == "gen":
        gen(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4])
    else:
        sys.exit(check(sys.argv[2], sys.argv[3]))
