#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""The FROZEN browser measurement -- round 78.

Rounds 61 and 67 proved the layout engine against a real Chromium: the
same cases through a foreign engine, box against box. That proof is worth
keeping. What is not worth keeping is the DEPENDENCY: a suite that only
passes when a 200 MB browser is installed is a suite that measures the
machine, not the code. Firn is supposed to stand on its own.

So the browser is asked ONCE, and its answer is written into this
repository as data:

    tools/layout/reference/cases.json     the boxes of tools/layout/cases
    tools/layout/reference/stack.json     the paint order probe points and
                                          the topmost element at each of them
    tools/layout/reference/realweb.json   the boxes of testdata/realweb

Every file carries a header saying WHICH browser said this, WHEN, and into
WHICH layout viewport -- without that a number is an opinion.

What this changes about the strength of the proof: nothing. The comparison
is still against a foreign engine that was written by other people from
the specification; only the moment of asking moved. What it does change is
that a deviation now has exactly one cause -- the Firn engine -- instead of
two, because the browser on the other side can no longer silently become a
different version between two runs.

Re-ask the browser (a newer version, more cases):

    bash tools/layout/run.sh --refresh-reference

Check against a live browser without touching the frozen data:

    bash tools/layout/run.sh --live-chromium
"""

import datetime
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
DIR = os.path.join(HERE, "reference")


def path(name):
    return os.path.join(DIR, name + ".json")


def exists(name):
    return os.path.exists(path(name))


def header(what, exe, window, viewport):
    """The provenance of a frozen measurement -- who, when, how big."""
    import chrome as chrome_mod
    return {
        "what": what,
        "chromium": chrome_mod.chromium_version(exe) or "unknown",
        "chromium_path": exe or "unknown",
        "created": datetime.date.today().isoformat(),
        "layout_viewport": [int(viewport[0]), int(viewport[1])],
        "window_size": [int(window[0]), int(window[1])],
        "refresh_with": "bash tools/layout/run.sh --refresh-reference",
    }


def save(name, head, data):
    os.makedirs(DIR, exist_ok=True)
    with open(path(name), "w", encoding="utf-8") as fh:
        json.dump({"_header": head, "data": data}, fh, indent=1,
                  sort_keys=True)
        fh.write("\n")
    return path(name)


def load(name):
    """-> (header, data).  Raises when the file is not there: a missing
    reference must be a FAILURE, never a silently skipped section."""
    p = path(name)
    if not os.path.exists(p):
        raise RuntimeError(
            "no frozen reference %s -- create it with "
            "`bash tools/layout/run.sh --refresh-reference`" % p)
    with open(p, encoding="utf-8") as fh:
        doc = json.load(fh)
    return doc.get("_header", {}), doc["data"]


def describe(head):
    return ("frozen reference: %s, %s, layout viewport %dx%d"
            % (head.get("chromium", "unknown"),
               head.get("created", "unknown"),
               head.get("layout_viewport", [0, 0])[0],
               head.get("layout_viewport", [0, 0])[1]))
