#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/js/report.py -- write tools/js/RESULTS.md out of the measurements.

Reads the per directory JSON files of `tools/js/run_all.sh` plus the JSON of
the parser harness and produces the table that the round reports. Nothing in
it is typed in by hand.
"""
import glob
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PER = os.path.join(ROOT, ".js-work", "per")
T262 = os.environ.get("T262", os.path.join(ROOT, ".js-work", "t262"))


def load(path):
    try:
        return json.load(open(path))
    except Exception:
        return None


def main():
    rows = []
    tot = passed = 0
    reasons = {}
    seen = set()
    for f in sorted(glob.glob(os.path.join(PER, "*.json"))):
        d = load(f)
        if not d:
            continue
        name = os.path.basename(f)[:-5].replace("_", "/", 1)
        rows.append((name, d["total"], d["passed"]))
        tot += d["total"]
        passed += d["passed"]
        seen.add(os.path.basename(f)[:-5])
        for k, v in d.get("reasons", {}).items():
            reasons[k] = reasons.get(k, 0) + v

    missing = []
    base = os.path.join(T262, "test")
    for group in ("language", "built-ins"):
        gdir = os.path.join(base, group)
        if not os.path.isdir(gdir):
            continue
        for d in sorted(os.listdir(gdir)):
            key = "%s_%s" % (group, d)
            if key in seen:
                continue
            n = sum(len([x for x in fs if x.endswith(".js")])
                    for _, _, fs in os.walk(os.path.join(gdir, d)))
            missing.append(("%s/%s" % (group, d), n))
            tot += n
    if missing:
        reasons["not-reached"] = sum(n for _, n in missing)

    parse = load(os.path.join(ROOT, ".js-work", "parse.json"))

    out = []
    out.append("# Round 74 -- the measurements of the JavaScript path")
    out.append("")
    out.append("Produced by `bash tools/js/run.sh` / `tools/js/report.py`.")
    out.append("Nothing here is typed in by hand.")
    out.append("")
    out.append("Reference: tc39/test262 @ "
               "`3655e7464de3d52643ecddd4b5f9f4f3e7f62398`, the subset of "
               "`testdata/test262/MANIFEST.md` (32,893 files).")
    out.append("")
    if parse:
        out.append("## The parser")
        out.append("")
        out.append("Does every case parse -- or fail to parse -- the way its "
                   "metadata says?")
        out.append("")
        out.append("| runs | passed | failed | quota |")
        out.append("|---:|---:|---:|---:|")
        out.append("| %d | %d | %d | %.2f%% |" %
                   (parse["total"], parse["passed"], parse["failed"],
                    100.0 * parse["passed"] / parse["total"]))
        out.append("")
    out.append("## The engine")
    out.append("")
    out.append("Every case really executed. A case that uses a feature this "
               "engine does not have counts as a FAILURE like any other; "
               "nothing is filtered.")
    out.append("")
    out.append("| runs | passed | failed | quota |")
    out.append("|---:|---:|---:|---:|")
    out.append("| %d | %d | %d | %.2f%% |" %
               (tot, passed, tot - passed,
                100.0 * passed / tot if tot else 0.0))
    out.append("")
    out.append("### The failures by cause")
    out.append("")
    out.append("| cause | cases |")
    out.append("|---|---:|")
    for k in sorted(reasons, key=lambda x: -reasons[x]):
        out.append("| %s | %d |" % (k, reasons[k]))
    out.append("")
    out.append("`unsupported-syntax` is a program that the parser rejects "
               "because the feature is deliberately absent (after round 74: "
               "`eval`, the `Function` constructor, modules -- the regular "
               "expressions moved into the engine in this round). `throw` is "
               "an exception the test did not expect -- usually a built in "
               "that does not exist. "
               "`async-incomplete` is a case with `flags: [async]` that ran "
               "through without ever printing `Test262:AsyncTestComplete`: "
               "its promise never settled. `wrong` is a case that ran "
               "through without the expected exception or delivered a wrong "
               "value: that is where the real bugs are.")
    out.append("")
    out.append("### Per directory")
    out.append("")
    out.append("| directory | runs | passed | quota |")
    out.append("|---|---:|---:|---:|")
    for name, t, p in sorted(rows):
        out.append("| %s | %d | %d | %.2f%% |" %
                   (name, t, p, 100.0 * p / t if t else 0.0))
    for name, n in missing:
        out.append("| %s | %d | 0 | not reached |" % (name, n))
    out.append("")
    path = os.path.join(ROOT, "tools", "js", "RESULTS.md")
    open(path, "w").write("\n".join(out) + "\n")
    print("engine: %d/%d = %.2f%%" % (passed, tot, 100.0 * passed / tot if tot else 0))
    if parse:
        print("parser: %d/%d = %.2f%%" % (parse["passed"], parse["total"],
                                          100.0 * parse["passed"] / parse["total"]))
    print("written: %s" % path)


main()
