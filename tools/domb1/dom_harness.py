#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Runner for the DOM and the style tree of round B1 (lib/dom/).

A WORKBENCH, NOT A PRODUCT: no DOM logic here. It reads the cases of
`tools/domb1/cases/*.dom`, packs them into jobs, calls the binary written
in Firn EXACTLY ONCE and compares the answers line by line.

An answer that is an empty line is written `<empty>` in the `-- want`
block.

The case format:

    == the name of the case
    -- html
    <the document>
    -- css            (optional: the USER stylesheet)
    ...
    -- ask
    <one question per line>
    -- want
    <one expected answer per line>

Every question has to yield exactly one answer line. A missing or extra
line is a failure, not a warning.

Usage: python3 tools/domb1/dom_harness.py <binary> [--show N] [--json FILE]
"""

import glob
import json
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CASES = os.path.join(ROOT, "tools", "domb1", "cases")


def load(path):
    cases = []
    name = None
    part = None
    cur = {}
    for line in open(path, encoding="utf-8").read().split("\n"):
        if line.startswith("== "):
            if name is not None:
                cases.append((name, cur))
            name = line[3:].strip()
            cur = {"html": [], "css": [], "ask": [], "want": []}
            part = None
        elif line.startswith("-- "):
            part = line[3:].strip()
        elif part is not None:
            cur[part].append(line)
    if name is not None:
        cases.append((name, cur))
    out = []
    for name, c in cases:
        for key in ("html", "css", "ask", "want"):
            while c[key] and c[key][-1] == "":
                c[key].pop()
        # An EMPTY answer line is written `<empty>` -- otherwise the
        # trailing blank line before the next case would swallow it.
        want = ["" if x == "<empty>" else x for x in c["want"]]
        out.append((os.path.basename(path), name,
                    "\n".join(c["html"]), "\n".join(c["css"]),
                    [x for x in c["ask"] if x.strip() != ""],
                    want))
    return out


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    binary = sys.argv[1]
    show = 0
    json_target = None
    args = sys.argv[2:]
    i = 0
    while i < len(args):
        if args[i] == "--show":
            show = int(args[i + 1])
            i += 2
        elif args[i] == "--json":
            json_target = args[i + 1]
            i += 2
        else:
            print("unknown option: %s" % args[i])
            return 2

    cases = []
    for path in sorted(glob.glob(os.path.join(CASES, "*.dom"))):
        cases += load(path)
    if not cases:
        print("NO CASES FOUND in %s" % CASES)
        return 1

    payload = b""
    for _, _, html, css, ask, _ in cases:
        for text in (html, css, "\n".join(ask)):
            raw = text.encode("utf-8")
            payload += struct.pack("<I", len(raw)) + raw

    p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=600)
    if p.returncode != 0:
        print("THE BINARY ENDED WITH %d" % p.returncode)
        print(p.stderr.decode("utf-8", "replace")[:2000])
        return 1
    blocks = p.stdout.decode("utf-8", "replace").split("#END\n")
    if blocks and blocks[-1] == "":
        blocks.pop()
    if len(blocks) != len(cases):
        print("WRONG NUMBER OF ANSWERS: %d blocks for %d cases"
              % (len(blocks), len(cases)))
        return 1

    passed = 0
    fails = []
    for (fname, name, html, css, ask, want), block in zip(cases, blocks):
        got = block.split("\n")
        # exactly ONE trailing empty piece comes from the last newline --
        # an empty ANSWER line has to survive.
        if got and got[-1] == "":
            got.pop()
        if got == want:
            passed += 1
        else:
            fails.append((fname, name, html, ask, want, got))

    print("%-18s %8s %8s" % ("file", "good", "total"))
    print("-" * 38)
    per_file = {}
    for (fname, name, _, _, _, _) in cases:
        per_file.setdefault(fname, [0, 0])[1] += 1
    for (fname, name, html, css, ask, want), block in zip(cases, blocks):
        got = block.split("\n")
        if got and got[-1] == "":
            got.pop()
        if got == want:
            per_file[fname][0] += 1
    for fname in sorted(per_file):
        good, total = per_file[fname]
        print("%-18s %8d %8d" % (fname, good, total))
    print("-" * 38)
    print("%-18s %8d %8d" % ("TOTAL", passed, len(cases)))

    if fails and show:
        for fname, name, html, ask, want, got in fails[:show]:
            print("\n--- %s / %s" % (fname, name))
            print("    html: %s" % html[:160])
            n = max(len(want), len(got))
            for k in range(n):
                w = want[k] if k < len(want) else "<missing>"
                g = got[k] if k < len(got) else "<missing>"
                mark = "  " if w == g else ">>"
                q = ask[k] if k < len(ask) else "?"
                print("   %s %-28s want %-30s got %s" % (mark, q, w, g))

    if json_target:
        with open(json_target, "w", encoding="utf-8") as fh:
            json.dump({"passed": passed, "total": len(cases)}, fh)
    return 0 if passed == len(cases) else 1


if __name__ == "__main__":
    sys.exit(main())
