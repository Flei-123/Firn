#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/js/harness_parse.py -- the PARSER against test262.

Every case of the chosen subset is handed to `build/jsparse` and the answer
is compared with the metadata of the test:

  * `negative: phase: parse` says the program MUST NOT parse,
  * everything else MUST parse.

The `flags` decide how often a case is run: `onlyStrict` once with a
prepended "use strict", `noStrict` once without, `raw` and `module` in their
own way, everything else BOTH ways -- exactly as the suite prescribes.

Nothing is filtered. A case the engine does not support counts as a
failure, like every other one.

Usage:  python3 tools/js/harness_parse.py <jsparse> [--json out.json]
                                          [--show N] [--list-dir DIR]
"""
import json
import os
import re
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DATA = os.environ.get("T262", os.path.join(ROOT, ".js-work", "t262"))

FRONT = re.compile(r"/\*---(.*?)---\*/", re.S)


def meta(src):
    """The YAML front matter of a test262 case -- the few keys that matter,
    read without a YAML library (the file format is fixed and simple)."""
    m = FRONT.search(src)
    if not m:
        return {}
    body = m.group(1)
    out = {}
    neg = re.search(r"^negative:\s*$(.*?)(?=^\S|\Z)", body, re.S | re.M)
    if neg:
        phase = re.search(r"phase:\s*(\S+)", neg.group(1))
        typ = re.search(r"type:\s*(\S+)", neg.group(1))
        out["negative"] = {"phase": phase.group(1) if phase else "",
                           "type": typ.group(1) if typ else ""}
    fl = re.search(r"^flags:\s*\[(.*?)\]", body, re.M)
    out["flags"] = [x.strip() for x in fl.group(1).split(",")] if fl else []
    inc = re.search(r"^includes:\s*\[(.*?)\]", body, re.M)
    out["includes"] = [x.strip() for x in inc.group(1).split(",")] if inc else []
    feat = re.search(r"^features:\s*\[(.*?)\]", body, re.M)
    out["features"] = [x.strip() for x in feat.group(1).split(",")] if feat else []
    return out


def cases(root):
    for base, _, files in os.walk(root):
        if "_FIXTURE" in base:
            continue
        for f in sorted(files):
            if not f.endswith(".js") or f.endswith("_FIXTURE.js"):
                continue
            yield os.path.join(base, f)


def variants(src, m):
    """(mode, text) per run of a case. mode 0 = script, 1 = module."""
    flags = m.get("flags", [])
    if "module" in flags:
        return [(1, src)]
    if "raw" in flags:
        return [(0, src)]
    out = []
    if "onlyStrict" not in flags:
        out.append((0, src))
    if "noStrict" not in flags:
        out.append((0, '"use strict";\n' + src))
    return out


def main():
    exe = sys.argv[1]
    args = sys.argv[2:]
    jsonout = None
    show = 0
    subdir = ""
    i = 0
    while i < len(args):
        if args[i] == "--json":
            jsonout = args[i + 1]
            i += 2
        elif args[i] == "--show":
            show = int(args[i + 1])
            i += 2
        elif args[i] == "--list-dir":
            subdir = args[i + 1]
            i += 2
        else:
            i += 1

    root = os.path.join(DATA, "test", subdir) if subdir else os.path.join(DATA, "test")
    jobs = []
    index = []
    for path in cases(root):
        src = open(path, encoding="utf-8").read()
        m = meta(src)
        neg = m.get("negative") or {}
        want_fail = neg.get("phase") in ("parse", "early")
        for mode, text in variants(src, m):
            data = text.encode("utf-8")
            jobs.append(struct.pack("<II", mode, len(data)) + data)
            index.append((path, want_fail, neg.get("type", "")))

    blob = b"".join(jobs)
    res = subprocess.run([exe], input=blob, stdout=subprocess.PIPE)
    lines = res.stdout.decode("utf-8", "replace").splitlines()
    if len(lines) != len(index):
        print("HARNESS ERROR: %d answers for %d runs" % (len(lines), len(index)))
        sys.exit(2)

    passed = 0
    failed = []
    for (path, want_fail, typ), line in zip(index, lines):
        got_fail = line.startswith("ERR")
        ok = (got_fail == want_fail)
        if ok:
            passed += 1
        else:
            failed.append((os.path.relpath(path, DATA), want_fail, line))

    total = len(index)
    print("runs        : %d" % total)
    print("passed      : %d" % passed)
    print("failed      : %d" % len(failed))
    print("quota       : %.2f%%" % (100.0 * passed / total if total else 0.0))
    if show:
        print("--- the first %d deviations ---" % show)
        for f in failed[:show]:
            kind = "should NOT parse" if f[1] else "should parse"
            print("  %s  (%s, got %s)" % (f[0], kind, f[2]))
    if jsonout:
        json.dump({"total": total, "passed": passed, "failed": len(failed),
                   "cases": [f[0] for f in failed]},
                  open(jsonout, "w"))
    sys.exit(0)


main()
