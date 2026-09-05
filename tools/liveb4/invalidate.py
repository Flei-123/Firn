#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/liveb4/invalidate.py -- what does NARROWING the recomputation buy,
and is the picture afterwards still the right one?

Two numbers per run, and neither means anything without the other:

  * the WORK: how many elements were styled and how many boxes were laid
    out, with the narrowing on and with it off. Same document, same
    mutations -- only `live_set_scoped` differs. With it off the numbers
    have to be the whole document, every time; that is the counter-check
    that the narrowing is really doing something and not just reporting
    smaller numbers.
  * the PICTURE: after every single mutation the same document is laid out
    from scratch and EVERY box is compared, x, y, w and h, as bit patterns.
    `bad` has to be 0. A narrowing that is fast and wrong is worth less
    than no narrowing at all.

Usage:  invalidate.py <binary> [--json]
"""
import json
import re
import struct
import subprocess
import sys

UA = open("lib/dom/ua.css").read() if False else ""


def u32(v):
    return struct.pack("<I", v)


def blob(b):
    return u32(len(b)) + b


def doc(n_rows, cols=6):
    """A document with WALLS in it. Every row sits in a box with a definite
    width and height and its own block formatting context, so a change
    inside one row cannot move anything outside it -- which is exactly the
    claim the layout scope makes and the counter-check has to confirm."""
    rows = []
    for i in range(n_rows):
        cells = "".join(
            '<span class="c c%d">cell %d %d</span>' % (j % 3, i, j)
            for j in range(cols))
        rows.append('<div class="wall"><div class="row r%d"><p>%s</p>'
                    '</div></div>' % (i, cells))
    return (
        "<!doctype html><html><head><style>"
        ".wall{width:380px;height:24px;overflow:hidden}"
        ".row{margin:2px;padding:1px}"
        ".c{padding:1px}"
        ".c0{color:#a00}.c1{color:#0a0}.c2{color:#00a}"
        "#box{width:400px;height:900px;overflow:hidden}"
        "#other{width:200px;height:80px}"
        "</style></head><body>"
        "<div id=other><p>outside</p></div>"
        '<div id=box>' + "".join(rows) + "</div>"
        "<div id=tail><p>after</p></div>"
        "</body></html>").encode()


def build(html, cmds, flags, vw=800, vh=600, author=b""):
    ua = (b"html,body,div,p{display:block}span{display:inline}"
          b"body{margin:8px}p{margin:1em 0}")
    out = u32(vw) + u32(vh) + blob(html) + blob(ua) + blob(author)
    out += u32(flags) + u32(len(cmds))
    for op, idx, val in cmds:
        out += u32(op) + u32(idx) + blob(val)
    return out


def run(binary, payload):
    p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=300)
    if p.returncode != 0:
        raise SystemExit("live_probe failed rc=%d: %s"
                         % (p.returncode, p.stderr.decode()[:400]))
    return p.stdout.decode()


def parse(text):
    steps = []
    load = {}
    for line in text.splitlines():
        d = dict(re.findall(r"(\w+)=\s*(\d+)", line))
        d = {k: int(v) for k, v in d.items()}
        if line.startswith("LOAD"):
            load = d
        elif line.startswith("STEP"):
            steps.append(d)
    return load, steps


def main():
    binary = sys.argv[1]
    as_json = "--json" in sys.argv
    html = doc(40)

    # element indices: 0=html 1=head 2=style? (style is an element) ...
    # We do not need to know them exactly -- the mutations pick elements
    # deep inside #box by their document order, which is stable.
    cmds = []
    for i in range(20):
        cmds.append((1, 40 + i * 5, b"color:#123456"))     # inline style
    for i in range(10):
        cmds.append((5, 61 + i * 5, b"c c1"))              # class

    scoped = parse(run(binary, build(html, cmds, 2)))
    plain = parse(run(binary, build(html, cmds, 1 | 2)))

    s_load, s_steps = scoped
    p_load, p_steps = plain
    bad = sum(s["bad"] for s in s_steps) + sum(s["bad"] for s in p_steps)

    s_styled = sum(s["styled"] for s in s_steps)
    p_styled = sum(s["styled"] for s in p_steps)
    s_laid = sum(s["laid"] for s in s_steps)
    p_laid = sum(s["laid"] for s in p_steps)
    s_vis = sum(s["visited"] for s in s_steps)
    p_vis = sum(s["visited"] for s in p_steps)
    n = len(s_steps)
    total_boxes = s_steps[0]["total"] if s_steps else 0
    elements = p_load.get("styled", 0)

    res = {
        "mutations": n,
        "elements": elements,
        "boxes": total_boxes,
        "styled_scoped": s_styled,
        "styled_full": p_styled,
        "visited_scoped": s_vis,
        "visited_full": p_vis,
        "laid_scoped": s_laid,
        "laid_full": p_laid,
        "scopes": s_steps[-1]["scope"] if s_steps else 0,
        "bad": bad,
        "us_style_scoped": sum(s["us_style"] for s in s_steps),
        "us_style_full": sum(s["us_style"] for s in p_steps),
        "us_layout_scoped": sum(s["us_layout"] for s in s_steps),
        "us_layout_full": sum(s["us_layout"] for s in p_steps),
    }
    if as_json:
        print(json.dumps(res))
        return 0 if bad == 0 else 1

    print("   document: %d elements, %d boxes, %d mutations"
          % (elements, total_boxes, n))
    print("   elements styled   narrowed %6d   full %6d   factor %.1f"
          % (s_styled, p_styled, p_styled / max(1, s_styled)))
    print("   nodes visited     narrowed %6d   full %6d   factor %.1f"
          % (s_vis, p_vis, p_vis / max(1, s_vis)))
    print("   boxes laid out    narrowed %6d   full %6d   factor %.1f"
          % (s_laid, p_laid, p_laid / max(1, s_laid)))
    print("   microseconds      narrowed %6d   full %6d   factor %.1f"
          % (res["us_style_scoped"] + res["us_layout_scoped"],
             res["us_style_full"] + res["us_layout_full"],
             (res["us_style_full"] + res["us_layout_full"])
             / max(1, res["us_style_scoped"] + res["us_layout_scoped"])))
    print("   layout walls hit  %d of %d" % (res["scopes"], n))
    print("   boxes whose geometry differs from a full layout: %d" % bad)
    if bad:
        print("   INVALIDATE FAIL: the narrowing changes the picture")
        return 1
    if s_styled >= p_styled:
        print("   INVALIDATE FAIL: narrowing did not reduce the work")
        return 1
    print("INVALIDATE OK: %d vs %d elements styled, %d boxes wrong"
          % (s_styled, p_styled, bad))
    return 0


if __name__ == "__main__":
    sys.exit(main())
