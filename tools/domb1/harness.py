#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Runner for the OFFICIAL html5lib tree construction tests (round B1).

A WORKBENCH, NOT A PRODUCT: this script contains NO parser logic. It turns
the `.dat` cases of `tests/data/html5lib/` into jobs, calls the binary
written in Firn EXACTLY ONCE and compares the answer line by line with the
expectation.

Rules of honesty -- they are the point of the whole exercise:
  * EVERY case of every `.dat` file is counted. Nothing is skipped and
    nothing is filtered.
  * `#document-fragment` cases run through the fragment parsing algorithm
    with their context element. They count like every other case.
  * The four `scripted_*.dat` files need a parser that EXECUTES scripts.
    They are counted as failures, because that is what they are here.
  * A `#KAPUTT` note from the binary is a failure.
  * What is compared is the whole tree, not a section of it.

Usage:
    python3 tools/domb1/harness.py <binary> [--dir DIR] [--json FILE]
                                   [--show N] [--only PATTERN]
                                   [--compare FILE]
"""

import glob
import json
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DATA = os.path.join(ROOT, "tests", "data", "html5lib")


def load_dat(path):
    """Reads a .dat file: a list of (data, document, fragment_context)."""
    cases = []
    # newline="" switches Python's universal newline translation OFF: a
    # `\r` in the data or in an expected text is a DELIBERATE byte
    # (`FOO&#x000D;ZOO`), not a line ending.
    with open(path, encoding="utf-8", errors="surrogateescape", newline="") as fh:
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
        for line in block.split("\n"):
            if line.startswith("#") and " " not in line.rstrip():
                cur = line[1:].strip()
                parts[cur] = []
                continue
            parts.setdefault(cur, []).append(line)
        data = "\n".join(parts.get("data", []))
        doc = parts.get("document", [])
        while doc and doc[-1] == "":
            doc.pop()
        context = "\n".join(parts.get("document-fragment", [])).strip() or None
        # WHATWG 13.2 knows a SCRIPTING FLAG. It is not script execution:
        # with the flag on, `noscript` holds raw text, with it off, markup.
        # The `.dat` format marks it per case; the `scripted_*` files are
        # script-on throughout.
        script = "script-on" in parts
        cases.append((data, "\n".join(doc), context, script))
    return cases


def load_all(pattern=None, dirname=None):
    all_cases = []
    for path in sorted(glob.glob(os.path.join(dirname or DATA, "*.dat"))):
        if pattern and pattern not in os.path.basename(path):
            continue
        base = os.path.basename(path)
        for i, (data, doc, context, script) in enumerate(load_dat(path)):
            all_cases.append((base, i, data, doc, context,
                              script or base.startswith("scripted_")))
    return all_cases


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    binary = sys.argv[1]
    json_target = None
    compare = None
    show = 0
    pattern = None
    dirname = None
    args = sys.argv[2:]
    i = 0
    while i < len(args):
        if args[i] == "--json":
            json_target = args[i + 1]
            i += 2
        elif args[i] == "--compare":
            compare = args[i + 1]
            i += 2
        elif args[i] == "--show":
            show = int(args[i + 1])
            i += 2
        elif args[i] == "--only":
            pattern = args[i + 1]
            i += 2
        elif args[i] == "--dir":
            dirname = args[i + 1]
            i += 2
        else:
            print("unknown option: %s" % args[i])
            return 2

    cases = load_all(pattern, dirname)
    if not cases:
        print("NO CASES FOUND in %s" % (dirname or DATA))
        return 1

    payload = b""
    for _, _, data, _, context, script in cases:
        raw = data.encode("utf-8", "surrogatepass")
        ctx = (context or "").encode("utf-8", "surrogatepass")
        payload += struct.pack("<I", len(raw)) + raw
        payload += struct.pack("<I", len(ctx)) + ctx
        payload += struct.pack("<I", 1 if script else 0)

    p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=1200)
    if p.returncode != 0:
        print("THE BINARY ENDED WITH %d" % p.returncode)
        print(p.stderr.decode("utf-8", "replace")[:2000])
        return 1
    raw = p.stdout.decode("utf-8", "surrogatepass")
    parts = raw.split("#ENDE\n")
    if parts and parts[-1] == "":
        parts.pop()
    if len(parts) != len(cases):
        print("WRONG NUMBER OF ANSWERS: %d blocks for %d cases"
              % (len(parts), len(cases)))
        return 1

    per_file = {}
    fails = []
    passed = 0
    good_names = []
    for (fname, idx, data, expected, context, script), answer in zip(cases, parts):
        got = answer.rstrip("\n")
        ok = (got == expected) and "#KAPUTT" not in answer
        st = per_file.setdefault(fname, [0, 0])
        st[1] += 1
        if ok:
            passed += 1
            st[0] += 1
            good_names.append("%s#%d" % (fname, idx))
        else:
            reason = "tree differs"
            if "#KAPUTT" in answer:
                reason = answer.splitlines()[0]
            fails.append((fname, idx, data, expected, got, reason, context))

    width = max(len(x) for x in per_file) + 2
    print("%-*s %8s %8s %8s" % (width, "file", "good", "total", "quota"))
    print("-" * (width + 28))
    for fname in sorted(per_file):
        good, total = per_file[fname]
        print("%-*s %8d %8d %7.2f %%" % (width, fname, good, total,
                                         100.0 * good / total))
    print("-" * (width + 28))
    total = len(cases)
    print("%-*s %8d %8d %7.2f %%" % (width, "TOTAL", passed, total,
                                     100.0 * passed / total))

    if show and fails:
        print("\nfirst %d failures:" % min(show, len(fails)))
        for fname, idx, data, expected, got, reason, context in fails[:show]:
            print("\n--- %s #%d (%s)" % (fname, idx, reason))
            print("    input: %r" % data)
            if context:
                print("    context: %r" % context)
            print("    expected:")
            for line in expected.split("\n"):
                print("      " + line)
            print("    got:")
            for line in got.split("\n"):
                print("      " + line)

    # The regression guard: no case that passed before may fail now.
    lost = []
    if compare and os.path.exists(compare):
        with open(compare, encoding="utf-8") as fh:
            before = set(json.load(fh).get("good", []))
        lost = sorted(before - set(good_names))
        if lost:
            print("\nREGRESSION: %d cases that used to pass now fail:" % len(lost))
            for name in lost[:20]:
                print("   " + name)

    if json_target:
        with open(json_target, "w", encoding="utf-8") as fh:
            json.dump({"passed": passed, "total": total,
                       "per_file": per_file, "good": good_names}, fh)
    if lost:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
