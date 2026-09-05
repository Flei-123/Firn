#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/paintb3/reftest.py -- ROUND B3 against the official REFERENCE
TESTS of the Web Platform Tests.

A reference test is two documents that have to look THE SAME.  They are
written so that the test file reaches its result the complicated way (the
property under test) and the reference file the simple way (a plain box in
the right place).  Comparing pixels is the whole point of them, and that
is exactly what a round about a rasteriser can do and what round B2 could
not.

------------------------------------------------------------- THE TRAP

A reference test is the easiest measurement in the world to cheat, and not
on purpose: AN ENGINE THAT DRAWS NOTHING PASSES EVERY ONE OF THEM.  Both
sides come out white, white equals white, the quota says 100 per cent.
Round K7B in the kernel is the same mistake in the other direction -- a
screen was called 87 per cent correct while every letter was missing,
because the 87 per cent were the black background.

So this harness never counts a pair as passed on equality alone.  A pair
counts only if

    * the two pictures match (exactly, or inside the `fuzzy` tolerance
      the test itself declares), AND
    * the picture is NOT EMPTY -- at least `--min-ink` pixels of the test
      rendering differ from the blank page it started as.

Pairs that match but are empty are counted and printed SEPARATELY, as
`vacuous`.  They are not in the quota.  The number that goes into the
documentation is the first one.

--------------------------------------------------------- WHAT IS NOT DONE

The harness does not run JavaScript, does not fetch anything over the
network and does not decode a second font per page: a page that asks for
Ahem gets Ahem, everything else gets the engine's own font, and the SAME
choice is made for the test and for its reference so that the comparison
stays a comparison.
"""
import argparse
import json
import os
import re
import struct
import subprocess
import sys
import time

import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def blk(b):
    return struct.pack("<I", len(b)) + b


def job(html, ua, author, font, vw, vh, flags):
    return (struct.pack("<II", vw, vh) + blk(html) + blk(ua) + blk(author)
            + blk(font) + struct.pack("<I", flags))


def read(path):
    with open(path, "rb") as f:
        return f.read()


def gather_css(base_dir, rel, text, seen):
    """The stylesheets a document links to, concatenated.  One level; an
    `@import` inside one of them is not followed, and that is written down
    rather than worked around."""
    out = []
    for h in re.findall(rb'<link[^>]+href=["\']([^"\']+)["\']', text, re.I):
        h = h.decode("utf-8", "replace").split("#")[0]
        if not h.endswith(".css"):
            continue
        if h.startswith("/"):
            p = os.path.join(base_dir, h[1:])
        else:
            p = os.path.normpath(os.path.join(base_dir, os.path.dirname(rel), h))
        if p in seen or not os.path.exists(p):
            continue
        seen.add(p)
        out.append(read(p))
    return b"\n".join(out)


def parse_fuzzy(text):
    """<meta name=fuzzy content="maxDifference=0-2;totalPixels=0-100">"""
    m = re.search(rb'<meta[^>]+name=["\']?fuzzy["\']?[^>]*>', text, re.I)
    if not m:
        return 0, 0
    c = re.search(rb'content=["\']([^"\']+)["\']', m.group(0))
    if not c:
        return 0, 0
    s = c.group(1).decode()
    if ":" in s:
        s = s.split(":", 1)[1]
    md, tp = 0, 0
    for part in s.split(";"):
        part = part.strip()
        if part.startswith("maxDifference"):
            md = int(part.split("=")[1].split("-")[-1])
        elif part.startswith("totalPixels"):
            tp = int(part.split("=")[1].split("-")[-1].replace(",", ""))
        elif "-" in part and not part.startswith("total"):
            md = int(part.split("-")[-1])
    return md, tp


def render(binary, html, ua, author, font, vw, vh, timings):
    j = job(html, ua, author, font, vw, vh, 2)
    p = subprocess.run([binary], input=j, capture_output=True, timeout=120)
    if p.returncode != 0:
        return None
    img = read_ppm(p.stdout)
    for line in p.stderr.decode("utf-8", "replace").splitlines():
        k = line.split()
        if len(k) == 2 and k[0].startswith("US-"):
            timings.setdefault(k[0], []).append(int(k[1]))
        elif len(k) == 2 and k[0] in ("CMDS", "INK"):
            timings.setdefault(k[0], []).append(int(k[1]))
    return img


def read_ppm(data):
    if not data.startswith(b"P6"):
        return None
    i = 2
    vals = []
    while len(vals) < 3:
        while i < len(data) and data[i:i + 1].isspace():
            i += 1
        if data[i:i + 1] == b"#":
            while i < len(data) and data[i] != 10:
                i += 1
            continue
        j = i
        while j < len(data) and not data[j:j + 1].isspace():
            j += 1
        vals.append(int(data[i:j]))
        i = j
    i += 1
    w, h, _ = vals
    return np.frombuffer(data[i:i + w * h * 3], dtype=np.uint8).reshape(h, w, 3)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("binary")
    ap.add_argument("--data", default=os.path.join(ROOT, "tests/data/wpt-ref"))
    ap.add_argument("--fonts", default=os.path.join(ROOT, "tests/data/fonts"))
    ap.add_argument("--width", type=int, default=800)
    ap.add_argument("--height", type=int, default=600)
    ap.add_argument("--min-ink", type=int, default=64,
                    help="a picture with fewer painted pixels is vacuous")
    ap.add_argument("--json", default=None)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--list-failures", type=int, default=0)
    args = ap.parse_args()

    ua = read(os.path.join(ROOT, "lib/dom/ua.css"))
    ahem = read(os.path.join(args.fonts, "Ahem.ttf"))
    sans = read(os.path.join(args.fonts, "FirnSans.ttf"))

    pairs = []
    with open(os.path.join(args.data, "pairs.txt")) as f:
        for line in f:
            a, b = line.split()
            pairs.append((a, b))
    if args.limit:
        pairs = pairs[:args.limit]

    timings = {}
    rows = []
    t_start = time.time()
    for rel, ref in pairs:
        tp = os.path.join(args.data, rel)
        rp = os.path.join(args.data, ref)
        if not (os.path.exists(tp) and os.path.exists(rp)):
            rows.append(dict(test=rel, state="missing"))
            continue
        thtml = read(tp)
        rhtml = read(rp)
        font = ahem if (b"Ahem" in thtml or b"Ahem" in rhtml) else sans
        tcss = gather_css(args.data, rel, thtml, set())
        rcss = gather_css(args.data, ref, rhtml, set())
        try:
            ta = render(args.binary, thtml, ua, tcss, font,
                        args.width, args.height, timings)
            ra = render(args.binary, rhtml, ua, rcss, font,
                        args.width, args.height, timings)
        except subprocess.TimeoutExpired:
            rows.append(dict(test=rel, state="timeout"))
            continue
        if ta is None or ra is None:
            rows.append(dict(test=rel, state="crash"))
            continue
        diff = np.abs(ta.astype(np.int16) - ra.astype(np.int16))
        maxd = int(diff.max())
        nbad = int((diff.max(axis=2) > 0).sum())
        md, tpx = parse_fuzzy(thtml)
        ok = maxd == 0 or (maxd <= md and nbad <= tpx)
        ink = int((np.abs(ta.astype(np.int16) - 255).max(axis=2) > 0).sum())
        state = "fail"
        if ok and ink >= args.min_ink:
            state = "pass"
        elif ok:
            state = "vacuous"
        rows.append(dict(test=rel, state=state, maxd=maxd, nbad=nbad, ink=ink))
    wall = time.time() - t_start

    npass = sum(1 for r in rows if r["state"] == "pass")
    nvac = sum(1 for r in rows if r["state"] == "vacuous")
    nfail = sum(1 for r in rows if r["state"] == "fail")
    nerr = len(rows) - npass - nvac - nfail
    total = len(rows)

    def avg(k):
        v = timings.get(k, [])
        return sum(v) / len(v) if v else 0

    print("   corpus        %d reference pairs (tests/data/wpt-ref)" % total)
    print("   passed        %d   (%.2f %%)  -- pictures equal AND not empty"
          % (npass, 100.0 * npass / total if total else 0))
    print("   vacuous       %d   equal but under %d painted pixels; NOT counted"
          % (nvac, args.min_ink))
    print("   failed        %d   pictures differ" % nfail)
    if nerr:
        print("   broken        %d   crashed, timed out or missing" % nerr)
    print("   ink           %d pixels set by glyphs over the whole corpus"
          % sum(timings.get("INK", [])))
    print("   commands      %d drawing commands over the whole corpus"
          % sum(timings.get("CMDS", [])))
    print("   time/page     layout %.1f ms, display list %.2f ms, "
          "raster %.1f ms" % (avg("US-LAYOUT") / 1000.0,
                              avg("US-LIST") / 1000.0, avg("US-PAINT") / 1000.0))
    print("   wall          %.1f s for %d renderings" % (wall, 2 * total))
    if args.list_failures:
        bad = [r for r in rows if r["state"] == "fail"]
        bad.sort(key=lambda r: -r.get("nbad", 0))
        for r in bad[:args.list_failures]:
            print("      %-60s %d px differ, worst %d"
                  % (r["test"], r.get("nbad", 0), r.get("maxd", 0)))
    print("B3-REF: %d / %d reference tests (%.2f %%), %d vacuous"
          % (npass, total, 100.0 * npass / total if total else 0, nvac))
    if args.json:
        json.dump(dict(rows=rows, passed=npass, total=total, vacuous=nvac,
                       timings={k: sum(v) / len(v) for k, v in timings.items()}),
                  open(args.json, "w"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
