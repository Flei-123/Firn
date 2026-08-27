#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Runner for the HTML tree construction from lib/browser/ (in Firn).

A WORKBENCH, NOT A PRODUCT: this script contains NO parser logic. It
turns the cases into jobs, calls the binary written in Firn
EXACTLY ONCE and compares the answer line by line with the
expectation.

Data format: the `.dat` format of the html5lib `tree-construction` tests.
Deliberately exactly that one and not one of our own -- once the original
data is available, this runner runs against it without a change (see
docs/ROUND54.md).

Rules of honesty:
  * EVERY case from all .dat files is counted. There is no
    skipping and there are no filters.
  * `#document-fragment` cases (parsing with a context element) are NOT
    implemented and count as a FAILURE, not as skipped.
  * A `#KAPUTT` note from the binary is a failure.
  * What is compared is the complete tree, not a section of it.

Usage: python3 tools/html/harness_tree.py <binary> [--json file] [--show N]
                                          [--only PATTERN]
"""

import glob
import json
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CASES = os.path.join(ROOT, "tools", "html", "cases")
GAPS = os.path.join(ROOT, "tools", "html", "gaps")


def load_dat(path):
    """Reads a .dat file: a list of (data, document, fragment_context)."""
    cases = []
    with open(path, encoding="utf-8") as fh:
        text = fh.read()
    for block in text.split("\n#data\n"):
        block = block.lstrip("\n")
        if block.startswith("#data\n"):
            block = block[len("#data\n"):]
        if not block.strip():
            continue
        parts = {}
        cur = "data"
        parts[cur] = []
        for ln2 in block.split("\n"):
            if ln2.startswith("#") and " " not in ln2.rstrip():
                cur = ln2[1:].strip()
                parts[cur] = []
                continue
            parts.setdefault(cur, []).append(ln2)
        data = "\n".join(parts.get("data", []))
        doc = parts.get("document", [])
        while doc and doc[-1] == "":
            doc.pop()
        context = "\n".join(parts.get("document-fragment", [])).strip() or None
        cases.append((data, "\n".join(doc), context))
    return cases


def load_all(pattern=None, dirname=None):
    all_cases = []
    for path in sorted(glob.glob(os.path.join(dirname or CASES, "*.dat"))):
        if pattern and pattern not in os.path.basename(path):
            continue
        for i, (data, doc, context) in enumerate(load_dat(path)):
            all_cases.append((os.path.basename(path), i, data, doc, context))
    return all_cases


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    binary = sys.argv[1]
    json_target = None
    show = 0
    pattern = None
    dirname = None
    args = sys.argv[2:]
    i = 0
    while i < len(args):
        if args[i] == "--json":
            json_target = args[i + 1]
            i += 2
        elif args[i] == "--show":
            show = int(args[i + 1])
            i += 2
        elif args[i] == "--only":
            pattern = args[i + 1]
            i += 2
        elif args[i] == "--gaps":
            dirname = GAPS
            i += 1
        elif args[i] == "--dir":
            dirname = args[i + 1]
            i += 2
        else:
            print("unknown option: %s" % args[i])
            return 2

    cases = load_all(pattern, dirname)
    if not cases:
        print("NO CASES FOUND in %s" % (dirname or CASES))
        return 1

    payload = b""
    for _, _, data, _, _ in cases:
        roh = data.encode("utf-8", "surrogatepass")
        payload += struct.pack("<I", len(roh)) + roh

    p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=600)
    if p.returncode != 0:
        print("THE BINARY ENDED WITH %d" % p.returncode)
        print(p.stderr.decode("utf-8", "replace")[:2000])
        return 1
    roh = p.stdout.decode("utf-8", "surrogatepass")
    parts = roh.split("#ENDE\n")
    if parts and parts[-1] == "":
        parts.pop()
    if len(parts) != len(cases):
        print("WRONG NUMBER OF ANSWERS: %d blocks for %d cases" % (len(parts), len(cases)))
        return 1

    per_file = {}
    fails = []
    passed = 0
    for (fname, idx, data, expected, context), answer in zip(cases, parts):
        got = answer.rstrip("\n")
        ok = (got == expected) and context is None and "#KAPUTT" not in answer
        st = per_file.setdefault(fname, [0, 0])
        st[1] += 1
        if ok:
            passed += 1
            st[0] += 1
        else:
            reason = "fragment not implemented" if context else "tree differs"
            if "#KAPUTT" in answer:
                reason = answer.splitlines()[0]
            fails.append((fname, idx, data, expected, got, reason))

    width = max(len(x) for x in per_file) + 2
    print("%-*s %8s %8s %8s" % (width, "file", "good", "total", "quota"))
    print("-" * (width + 28))
    for fname in sorted(per_file):
        good, total = per_file[fname]
        print("%-*s %8d %8d %7.2f %%" % (width, fname, good, total, 100.0 * good / total))
    print("-" * (width + 28))
    total = len(cases)
    print("%-*s %8d %8d %7.2f %%" % (width, "TOTAL", passed, total,
                                     100.0 * passed / total))

    if show and fails:
        print("\nfirst %d failures:" % min(show, len(fails)))
        for fname, idx, data, expected, got, reason in fails[:show]:
            print("\n--- %s #%d (%s)" % (fname, idx, reason))
            print("    input: %r" % data)
            print("    expected:")
            for ln in expected.split("\n"):
                print("      " + ln)
            print("    got:")
            for ln in got.split("\n"):
                print("      " + ln)

    if json_target:
        with open(json_target, "w", encoding="utf-8") as fh:
            json.dump({"passed": passed, "total": total,
                       "per_file": per_file}, fh, indent=1)
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
