#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Round B2: the layout engine against the OFFICIAL Web Platform Tests.

THE MEASURING STICK, AND WHY IT CAN BE READ WITHOUT A RASTERISER

Most of the WPT css/ area consists of reftests: a test file and a
reference file are rendered and the PIXELS are compared. An engine that
cannot paint yet -- round B2 stops at the box tree -- cannot run those.

But a large part of the suite is self-describing in a different way: the
test carries its expectation IN the markup and `resources/check-layout-th.js`
checks it:

    <div data-expected-width="100" data-offset-y="20">

The values compared are the CSSOM View accessors -- `offsetWidth`,
`offsetHeight`, `offsetLeft`, `offsetTop`, `clientWidth`, `clientHeight`,
`scrollWidth`, `scrollHeight`, `getBoundingClientRect()`, the computed
`display` and the used margins and paddings. Those are POSITION AND SIZE,
not pixels; they are exactly what a layout engine produces. The
expectations are written by the CSS working group and by the engineers of
the other browsers, not by the author of this code -- which is the whole
point of the exercise.

This harness therefore does what check-layout-th.js does, to the letter:

  * it reads the same attributes (the list in `DATA_KEYS` is the
    `validData` set of check-layout-th.js),
  * it applies the same tolerance: `assert_tolerance` in that file fails
    only when `Math.abs(actual - expected) >= 1`, so a difference below one
    pixel passes. That is the suite's rule, not a softening of it -- it
    exists because a browser reports these accessors as ROUNDED integers
    while a layout engine computes in fractions.
  * a test counts as PASSED only if every one of its checks passes.

`lib/layout/b2_main.fi` prints those accessors per element; the comparison
below is a subtraction.

WHICH TESTS ARE IN THE CORPUS

Everything harvested lies in `tests/data/wpt-css/` (see PROVENANCE.md
there): every file of the WPT directories `css/css-flexbox`, `css/CSS2`,
`css/css-box`, `css/css-sizing`, `css/css-position` and `css/css-align`
that includes `check-layout-th.js`. Out of those, three groups are put
aside MECHANICALLY, and each of them is counted and printed, so that
nobody has to take the word of this file for it:

  script   the test builds or changes the document from JavaScript. The
           JS engine of lib/js/ is not wired to the DOM (that is a round
           of its own), so the document under test never comes into being.
  grid     the test uses CSS grid. Grid is the NEXT round; B2 is box
           model, normal flow, inline flow, floats, positioning, flexbox.
  vertical the test uses a vertical writing mode or right-to-left
           direction. css-writing-modes is a module of its own and this
           engine has no writing modes; the tests are measured and
           reported SEPARATELY instead of being hidden.

The remaining set is "corpus B2". Both numbers -- corpus B2 and the whole
harvest without the script group -- are printed and both go into the
documentation.

Usage:
    python3 tools/layoutb2/harness.py <binary> [--json out.json]
                                      [--show N] [--only SUBSTRING]
                                      [--group NAME] [--verbose]
"""

import argparse
import json
import os
import re
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
DATA = os.path.join(ROOT, "tests", "data", "wpt-css")
UA = os.path.join(ROOT, "tools", "layout", "ua.css")

# The layout viewport the expectations were written for. WPT runs its
# layout tests in a window of 800x600 css pixels (the default of the wpt
# runner), and the tests that depend on it say so in their numbers.
VIEWPORT_W = 800
VIEWPORT_H = 600

# The `validData` set of resources/check-layout-th.js, without the two
# entries that are not geometry (`data-anchor-polyfill`, `data-test*`).
DATA_KEYS = {
    "data-expected-width": ("num", "offsetWidth"),
    "data-expected-height": ("num", "offsetHeight"),
    "data-offset-x": ("num", "offsetLeft"),
    "data-offset-y": ("num", "offsetTop"),
    "data-expected-client-width": ("num", "clientWidth"),
    "data-expected-client-height": ("num", "clientHeight"),
    "data-expected-scroll-width": ("num", "scrollWidth"),
    "data-expected-scroll-height": ("num", "scrollHeight"),
    "data-expected-bounding-client-rect-width": ("num", "rectWidth"),
    "data-expected-bounding-client-rect-height": ("num", "rectHeight"),
    "data-total-x": ("num", "totalX"),
    "data-total-y": ("num", "totalY"),
    "data-expected-display": ("str", "display"),
    "data-expected-padding-top": ("px", "paddingTop"),
    "data-expected-padding-bottom": ("px", "paddingBottom"),
    "data-expected-padding-left": ("px", "paddingLeft"),
    "data-expected-padding-right": ("px", "paddingRight"),
    "data-expected-margin-top": ("px", "marginTop"),
    "data-expected-margin-bottom": ("px", "marginBottom"),
    "data-expected-margin-left": ("px", "marginLeft"),
    "data-expected-margin-right": ("px", "marginRight"),
}

# The tolerance of check-layout-th.js: a difference of less than one pixel
# passes. For the used margins and paddings, which that file compares as
# STRINGS, a sixty-fourth of a pixel is used instead -- the grid the whole
# engine computes in.
TOLERANCE = 1.0
PX_EPS = 1.0 / 64.0

GRID_RE = re.compile(
    r"display\s*:\s*(inline-)?grid|grid-template|grid-auto-|"
    r"\bgrid-column\b|\bgrid-row\b|display\s*:\s*(inline-)?ruby", re.I)
VERTICAL_RE = re.compile(
    r"writing-mode\s*:\s*(vertical|sideways)|direction\s*:\s*rtl|"
    r"\bdir\s*=\s*[\"']?rtl", re.I)
SCRIPT_RE = re.compile(r"<script(?![^>]*\bsrc\b)[^>]*>(.*?)</script>", re.S | re.I)
LINK_RE = re.compile(r"<link[^>]*>", re.I)
HREF_RE = re.compile(r"href\s*=\s*[\"']([^\"']+)[\"']", re.I)
REL_RE = re.compile(r"rel\s*=\s*[\"']?stylesheet", re.I)


def classify_script(text):
    """True when an inline script does more than call checkLayout()."""
    for body in SCRIPT_RE.findall(text):
        stripped = re.sub(r"checkLayout\s*\([^;]*\)\s*;?", "", body)
        stripped = re.sub(r"//[^\n]*", "", stripped)
        stripped = re.sub(r"/\*.*?\*/", "", stripped, flags=re.S)
        stripped = stripped.replace("'use strict';", "")
        if stripped.strip():
            return True
    return False


def group_of(path, text):
    if classify_script(text):
        return "script"
    if GRID_RE.search(text):
        return "grid"
    if VERTICAL_RE.search(text):
        return "vertical"
    return "b2"


def linked_sheets(path, text):
    """The text of every <link rel=stylesheet> the document pulls in."""
    out = []
    rel = os.path.relpath(path, DATA)              # css/css-flexbox/x.html
    for tag in LINK_RE.findall(text):
        if not REL_RE.search(tag):
            continue
        m = HREF_RE.search(tag)
        if not m:
            continue
        href = m.group(1).split("#")[0].split("?")[0]
        if href.startswith("http"):
            continue
        if href.startswith("/"):
            target = os.path.join(DATA, href.lstrip("/"))
        else:
            target = os.path.normpath(
                os.path.join(DATA, os.path.dirname(rel), href))
        if os.path.exists(target):
            out.append(open(target, encoding="utf-8", errors="replace").read())
    return out


def collect():
    tests = []
    for dirpath, _dirs, files in os.walk(os.path.join(DATA, "css")):
        for f in sorted(files):
            if not f.endswith(".html"):
                continue
            p = os.path.join(dirpath, f)
            text = open(p, encoding="utf-8", errors="replace").read()
            if "check-layout-th.js" not in text:
                continue
            tests.append({
                "path": p,
                "name": os.path.relpath(p, DATA),
                "text": text,
                "group": group_of(p, text),
            })
    tests.sort(key=lambda t: t["name"])
    return tests


# ------------------------------------------------------------- the engine

def encode_job(html, ua, author, vw=VIEWPORT_W, vh=VIEWPORT_H):
    def blob(text):
        raw = text.encode("utf-8")
        return struct.pack("<I", len(raw)) + raw
    return struct.pack("<II", vw, vh) + blob(html) + blob(ua) + blob(author)


def run_engine(binary, jobs, timeout=300, extra=()):
    payload = b"".join(encode_job(h, u, a) for h, u, a in jobs)
    proc = subprocess.run([binary] + list(extra), input=payload,
                          stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, timeout=timeout)
    if proc.returncode != 0:
        raise RuntimeError("engine exited with %d: %s"
                           % (proc.returncode, proc.stderr[-400:]))
    blocks, current = [], []
    for line in proc.stdout.decode("utf-8", "replace").splitlines():
        if line == "#END":
            blocks.append(current)
            current = []
        else:
            current.append(line)
    return blocks


def parse_block(lines):
    """One document: index -> the accessors and the data- attributes."""
    elems = {}
    broken = False
    for line in lines:
        if line.startswith("#BROKEN"):
            broken = True
            continue
        parts = line.split(" ")
        if len(parts) < 2:
            continue
        idx = parts[1]
        e = elems.setdefault(idx, {"tag": "?", "attrs": {}, "v": {}})
        if parts[0] == "E":
            e["tag"] = parts[3] if len(parts) > 3 else "?"
            e["parent"] = parts[2]
        elif parts[0] == "G" and len(parts) >= 14:
            f = [float(x) for x in parts[2:14]]
            e["v"].update({
                "offsetLeft": f[0], "offsetTop": f[1],
                "offsetWidth": f[2], "offsetHeight": f[3],
                "clientLeft": f[4], "clientTop": f[5],
                "clientWidth": f[6], "clientHeight": f[7],
                "rectWidth": f[8], "rectHeight": f[9],
                "scrollWidth": f[10], "scrollHeight": f[11],
            })
            e["v"]["totalX"] = f[4] + f[0]
            e["v"]["totalY"] = f[5] + f[1]
        elif parts[0] == "S" and len(parts) >= 11:
            e["v"]["display"] = parts[2]
            names = ["marginTop", "marginRight", "marginBottom", "marginLeft",
                     "paddingTop", "paddingRight", "paddingBottom",
                     "paddingLeft"]
            for i, nm in enumerate(names):
                e["v"][nm] = float(parts[3 + i])
        elif parts[0] == "A" and len(parts) >= 4:
            e["attrs"][parts[2]] = " ".join(parts[3:])
    return elems, broken


def check_block(elems, broken):
    """The comparison of check-layout-th.js, check for check."""
    checks, fails = 0, []
    for idx, e in sorted(elems.items(), key=lambda kv: int(kv[0])):
        for attr, expected in e["attrs"].items():
            spec = DATA_KEYS.get(attr)
            if spec is None:
                continue                      # data-test*, unknown: ignored
            kind, key = spec
            actual = e["v"].get(key)
            checks += 1
            if broken or actual is None:
                fails.append((e["tag"], attr, expected, "n/a"))
                continue
            if kind == "str":
                if str(actual) != expected.strip():
                    fails.append((e["tag"], attr, expected, actual))
            else:
                try:
                    want = float(expected.strip())
                except ValueError:
                    fails.append((e["tag"], attr, expected, actual))
                    continue
                eps = TOLERANCE if kind == "num" else PX_EPS
                if not abs(actual - want) < eps:
                    fails.append((e["tag"], attr, expected, round(actual, 4)))
    return checks, fails


# ----------------------------------------------------------------- the run

def measure(binary, tests, ua_text, chunk=20, verbose=False):
    results = []
    for start in range(0, len(tests), chunk):
        part = tests[start:start + chunk]
        jobs = [(t["text"], ua_text,
                 "\n".join(linked_sheets(t["path"], t["text"])))
                for t in part]
        try:
            blocks = run_engine(binary, jobs)
            if len(blocks) != len(part):
                raise RuntimeError("engine returned %d blocks for %d jobs"
                                   % (len(blocks), len(part)))
        except Exception as exc:
            # One bad document must not take twenty with it: repeat the
            # chunk one by one so the failure lands on the test that
            # caused it.
            if len(part) > 1:
                results.extend(measure(binary, part, ua_text, 1, verbose))
                continue
            results.append({"name": part[0]["name"], "group": part[0]["group"],
                            "checks": 0,
                            "fails": [("engine", str(exc)[:120], "", "")],
                            "crash": True})
            if verbose:
                print("   CRASH %s: %s" % (part[0]["name"], str(exc)[:120]))
            continue
        for t, lines in zip(part, blocks):
            elems, broken = parse_block(lines)
            checks, fails = check_block(elems, broken)
            results.append({"name": t["name"], "group": t["group"],
                            "checks": checks, "fails": fails,
                            "crash": False, "broken": broken})
    return results


def reflow_check(binary, tests, ua_text, chunk=20):
    """THE PROOF OF THE SPLIT: a second layout of the same tree at another
    width and back has to give the very same tree as the first one.

    The engine keeps the box tree, the styles and the intrinsic widths
    across a reflow and throws every used value away
    (`flow.relayout_document`). If anything the window width decides
    survived by accident, the second run at 800 differs from the first --
    and this comparison is the only place where that shows up."""
    same, differ, first_bad = 0, 0, []
    for start in range(0, len(tests), chunk):
        part = tests[start:start + chunk]
        jobs = [(t["text"], ua_text,
                 "\n".join(linked_sheets(t["path"], t["text"])))
                for t in part]
        try:
            plain = run_engine(binary, jobs)
            again = run_engine(binary, jobs, extra=("--reflow",))
        except Exception as exc:
            if len(part) == 1:
                differ += 1
                first_bad.append((part[0]["name"], str(exc)[:100]))
                continue
            for one in part:
                sub = reflow_check(binary, [one], ua_text, 1)
                same += sub[0]
                differ += sub[1]
            continue
        for t, a, b in zip(part, plain, again):
            if a == b:
                same += 1
            else:
                differ += 1
                if len(first_bad) < 5:
                    diff = [(x, y) for x, y in zip(a, b) if x != y][:2]
                    first_bad.append((t["name"], diff))
    print("REFLOW: %d of %d documents identical after 800 -> 400 -> 800"
          % (same, same + differ))
    for name, d in first_bad:
        print("   DIFFERS %s %s" % (name, d))
    return 0 if differ == 0 else 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("binary")
    ap.add_argument("--json")
    ap.add_argument("--show", type=int, default=0)
    ap.add_argument("--only")
    ap.add_argument("--group")
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--reflow-check", action="store_true",
                    help="lay every document out three times (800, 400, 800) "
                         "and compare against the single layout")
    args = ap.parse_args()

    ua_text = open(UA, encoding="utf-8").read()
    tests = collect()
    if args.only:
        tests = [t for t in tests if args.only in t["name"]]
    if args.group:
        tests = [t for t in tests if t["group"] == args.group]
    if not tests:
        print("no tests found")
        return 2

    if args.reflow_check:
        return reflow_check(args.binary, tests, ua_text)

    results = measure(args.binary, tests, ua_text, verbose=args.verbose)

    groups = {}
    for r in results:
        g = groups.setdefault(r["group"], {"n": 0, "pass": 0, "checks": 0,
                                           "check_pass": 0})
        g["n"] += 1
        g["checks"] += r["checks"]
        g["check_pass"] += r["checks"] - len(r["fails"])
        if r["checks"] and not r["fails"]:
            g["pass"] += 1

    def line(label, g):
        if not g["n"]:
            return
        print("   %-22s %5d / %-5d tests  %6.2f %%   %6d / %-6d checks"
              % (label, g["pass"], g["n"], 100.0 * g["pass"] / g["n"],
                 g["check_pass"], g["checks"]))

    order = ["b2", "vertical", "grid", "script"]
    print("WPT css layout tests (check-layout-th.js), viewport %dx%d:"
          % (VIEWPORT_W, VIEWPORT_H))
    for k in order:
        if k in groups:
            line(k, groups[k])
    runnable = {"n": 0, "pass": 0, "checks": 0, "check_pass": 0}
    for k in ("b2", "vertical", "grid"):
        if k in groups:
            for f in runnable:
                runnable[f] += groups[k][f]
    line("all but script", runnable)

    if args.show:
        shown = 0
        for r in results:
            if r["fails"] and shown < args.show:
                shown += 1
                print("\n-- %s (%s)" % (r["name"], r["group"]))
                for tag, attr, want, got in r["fails"][:6]:
                    print("     <%s> %s: expected %s, got %s"
                          % (tag, attr, want, got))

    if args.json:
        with open(args.json, "w") as fh:
            json.dump({"results": results, "groups": groups}, fh, indent=1)

    b2 = groups.get("b2", {"pass": 0, "n": 0})
    print("B2-WPT: %d / %d tests of corpus B2 (%.2f %%)"
          % (b2["pass"], b2["n"], 100.0 * b2["pass"] / max(1, b2["n"])))
    return 0


if __name__ == "__main__":
    sys.exit(main())
