#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/paintb3/harvest.py -- fetch the reference tests of round B3 out of
the Web Platform Tests.

RUN BY HAND, NEVER FROM test.sh.  The files it fetches live in the
repository (tests/data/wpt-ref/), so the acceptance run opens no socket.
The rule by which they were picked is written into PROVENANCE.md there and
is repeated here so that anybody can run it again and get the same set:

    every file DIRECTLY IN  css/css-backgrounds/  and  css/css-color/
    that carries  <link rel="match" href="...">  -- together with the
    reference file it names and every stylesheet either of them links to.

Nothing is picked out by hand, and nothing is left out because it looked
hard.  A reference test whose expectation this engine cannot reach is a
FAILING test in the quota, not a missing file.
"""
import argparse
import json
import os
import re
import sys
import urllib.request

RAW = "https://raw.githubusercontent.com/web-platform-tests/wpt/master/"
API = "https://api.github.com/repos/web-platform-tests/wpt/contents/"
DIRS = ["css/css-backgrounds", "css/css-color"]


CACHE = ".b3-work/wptcache"


def get(url, binary=False):
    req = urllib.request.Request(url, headers={"User-Agent": "firn-b3"})
    with urllib.request.urlopen(req, timeout=60) as r:
        d = r.read()
    return d if binary else d.decode("utf-8", "replace")


def cached(rel):
    """The download is resumable: every file fetched lands in a cache
    beside the work directory, so a run that is interrupted -- and over
    eight hundred files one will be -- picks up where it stopped."""
    p = os.path.join(CACHE, rel)
    if os.path.exists(p):
        with open(p, "rb") as f:
            return f.read().decode("utf-8", "replace")
    try:
        d = get(RAW + rel)
    except Exception:
        return None
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "wb") as f:
        f.write(d.encode())
    return d


def listing(path):
    return json.loads(get(API + path))


def save(root, rel, data):
    p = os.path.join(root, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "wb") as f:
        f.write(data if isinstance(data, bytes) else data.encode())


def resolve(base_rel, href):
    href = href.split("#")[0].split("?")[0]
    if not href:
        return None
    if href.startswith("http"):
        return None
    if href.startswith("/"):
        return href[1:]
    return os.path.normpath(os.path.join(os.path.dirname(base_rel), href))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="tests/data/wpt-ref")
    ap.add_argument("--limit", type=int, default=0)
    args = ap.parse_args()

    pairs = []
    extra = set()
    for d in DIRS:
        for entry in listing(d):
            if entry["type"] != "file" or not entry["name"].endswith(".html"):
                continue
            rel = d + "/" + entry["name"]
            try:
                text = get(RAW + rel)
            except Exception as e:
                print("  skip %s (%s)" % (rel, e))
                continue
            m = re.search(r'<link[^>]+rel=["\']?match["\']?[^>]*>', text,
                          re.I)
            if not m:
                continue
            h = re.search(r'href=["\']([^"\']+)["\']', m.group(0))
            if not h:
                continue
            ref = resolve(rel, h.group(1))
            if ref is None:
                continue
            pairs.append((rel, ref, text))
            if args.limit and len(pairs) >= args.limit:
                break
        if args.limit and len(pairs) >= args.limit:
            break

    print("found %d reference pairs" % len(pairs))
    kept = []
    for rel, ref, text in pairs:
        try:
            reftext = get(RAW + ref)
        except Exception as e:
            print("  no reference for %s (%s)" % (rel, e))
            continue
        save(args.out, rel, text)
        save(args.out, ref, reftext)
        kept.append((rel, ref))
        for base, t in ((rel, text), (ref, reftext)):
            for h in re.findall(r'<link[^>]+href=["\']([^"\']+)["\']', t):
                if not h.endswith(".css"):
                    continue
                r2 = resolve(base, h)
                if r2:
                    extra.add(r2)
    for r2 in sorted(extra):
        p = os.path.join(args.out, r2)
        if os.path.exists(p):
            continue
        try:
            save(args.out, r2, get(RAW + r2))
        except Exception as e:
            print("  no stylesheet %s (%s)" % (r2, e))
    with open(os.path.join(args.out, "pairs.txt"), "w") as f:
        for rel, ref in kept:
            f.write("%s %s\n" % (rel, ref))
    print("kept %d pairs, %d stylesheets" % (len(kept), len(extra)))


if __name__ == "__main__":
    sys.exit(main())
