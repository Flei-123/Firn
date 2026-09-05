#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/liveb4/harvest.py -- fetch the DOM and EVENT tests of round B4 out
of the Web Platform Tests.

RUN BY HAND, NEVER FROM test.sh. What it fetches lands in the repository
(tests/data/wpt-dom/), so the acceptance run opens no socket. The rule by
which the files are picked is written into PROVENANCE.md there and repeated
here so anybody can run it again and get the same set:

    every *.html DIRECTLY IN

        dom/nodes  dom/events  dom/traversal  dom/lists  dom/ranges
        dom/abort

    that includes  /resources/testharness.js  -- together with every
    script and stylesheet it references, and with resources/testharness.js
    itself.

Nothing is picked out by hand and nothing is left out because it looked
hard. A test this engine cannot pass is a FAILING test in the quota, not a
missing file. Tests that need a second browsing context (an iframe, a
worker, a window opened with `window.open`) are fetched like the rest and
fail; they are counted in the "cannot run" column of docs/ROUNDB4.md,
because pretending they do not exist would be the same lie as deleting
them.
"""
import argparse
import json
import os
import posixpath
import re
import sys
import urllib.request

RAW = "https://raw.githubusercontent.com/web-platform-tests/wpt/master/"
API = "https://api.github.com/repos/web-platform-tests/wpt/contents/"
DIRS = ["dom/nodes", "dom/events", "dom/traversal", "dom/lists",
        "dom/ranges", "dom/abort"]
OUT = "tests/data/wpt-dom"
EXTRA = ["resources/testharness.js", "dom/common.js"]

SRC_RE = re.compile(rb"""<script[^>]*\ssrc\s*=\s*["']([^"']+)["']""",
                    re.IGNORECASE)
CSS_RE = re.compile(
    rb"""<link[^>]*\srel\s*=\s*["']?stylesheet["']?[^>]*\shref\s*=\s*"""
    rb"""["']([^"']+)["']""", re.IGNORECASE)


def get(url, binary=True):
    req = urllib.request.Request(url, headers={"User-Agent": "firn-b4"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def save(rel, data):
    path = os.path.join(OUT, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)


def have(rel):
    return os.path.exists(os.path.join(OUT, rel))


def resolve(base_rel, href):
    href = href.split("#")[0]
    if not href:
        return None
    if href.startswith("http:") or href.startswith("https:"):
        return None
    if href.startswith("/"):
        return href[1:]
    return posixpath.normpath(posixpath.join(posixpath.dirname(base_rel),
                                             href))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int, default=0,
                    help="at most this many test files (0 = all)")
    a = ap.parse_args()

    # The recursive tree of the whole repository comes back TRUNCATED --
    # wpt has more than 100000 blobs. So the directories are listed one by
    # one; that is six requests instead of one and cannot silently lose a
    # file, which the truncated tree would.
    print("listing the directories ...", flush=True)
    files = set()
    tests = []
    for d in DIRS:
        entries = json.loads(get(API + d).decode())
        for e in entries:
            if e["type"] != "file":
                continue
            files.add(e["path"])
            if e["path"].endswith(".html"):
                tests.append(e["path"])
        print("  %-16s %d entries" % (d, len(entries)), flush=True)
    tests.sort()
    print("%d candidate files in %s" % (len(tests), ", ".join(DIRS)))

    kept = []
    support = set()
    for i, rel in enumerate(tests):
        if a.limit and len(kept) >= a.limit:
            break
        if have(rel):
            data = open(os.path.join(OUT, rel), "rb").read()
        else:
            try:
                data = get(RAW + rel)
            except Exception as ex:
                print("  skip %s (%s)" % (rel, ex))
                continue
        if b"/resources/testharness.js" not in data:
            continue
        save(rel, data)
        kept.append(rel)
        for m in SRC_RE.findall(data) + CSS_RE.findall(data):
            r = resolve(rel, m.decode("utf-8", "replace"))
            if r and not r.startswith(".."):
                support.add(r)
        if len(kept) % 25 == 0:
            print("  %d kept (%d/%d looked at)" % (len(kept), i + 1,
                                                   len(tests)), flush=True)

    for r in EXTRA:
        support.add(r)
    print("%d tests, %d support files" % (len(kept), len(support)))
    got = 0
    for r in sorted(support):
        if have(r):
            got += 1
            continue
        try:
            save(r, get(RAW + r))
            got += 1
        except Exception as ex:
            print("  support missing: %s (%s)" % (r, ex))
    print("%d support files present" % got)

    with open(os.path.join(OUT, "PROVENANCE.md"), "w") as f:
        f.write(PROV % (", ".join(DIRS), len(kept), got))
    print("wrote", os.path.join(OUT, "PROVENANCE.md"))
    return 0


PROV = """# tests/data/wpt-dom -- the official DOM tests of round B4

These files are the **Web Platform Tests**. They were downloaded from

    https://github.com/web-platform-tests/wpt
    branch `master`, on 26 August 2026

with `raw.githubusercontent.com`, path for path, by
`tools/liveb4/harvest.py`. The directory layout is the one of the WPT
repository, so a `<script src="/resources/testharness.js">` in a test
resolves the same way it does there.

## Which files, and by which rule

Every `*.html` DIRECTLY IN

    %s

that includes `/resources/testharness.js`: **%d files**, together with
**%d** support files (the scripts and stylesheets they reference, and
`resources/testharness.js` itself). Nothing was picked by hand and nothing
was left out because it looked hard.

`testharness.js` is what makes them usable here: it is the OFFICIAL
harness, unmodified, and it runs inside this engine. Its result is read
through `add_completion_callback`, the same interface a real browser
runner uses. A test this engine cannot pass is a FAILING test in the
quota, not a missing file.

## What is deliberately NOT counted

A test whose harness never reaches `add_completion_callback` -- because it
needs a second browsing context, a worker, `fetch`, or an API this round
does not have at all -- is counted as **could not run** and is reported
separately from the failures. Counting it as a failure would be honest
too; counting it as a pass would not, and neither would leaving it out of
the corpus.
"""


if __name__ == "__main__":
    sys.exit(main())
