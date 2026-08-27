#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Runner for the CSS syntax parser from lib/css/ (in Firn).

A WORKBENCH, NOT A PRODUCT: this script contains NO parser logic. It turns
the cases of css-parsing-tests into jobs, calls the binary written in Firn
EXACTLY ONCE and compares its answer with the expectation -- JSON against
JSON.

Rules of honesty:
  * EVERY case of all ten stored files is counted. There is no skipping
    and there are no filters.
  * A case whose mode the binary does not know counts as a FAILURE.
  * The comparison is the full structure, not a section of it. Numbers are
    compared by value (1000 == 1000.0), everything else exactly.

Usage: python3 tools/css/harness_syntax.py <binary> [--json file] [--show N]
                                           [--only FILE]
"""

import json
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DATA = os.path.join(ROOT, "testdata", "css-parsing-tests")

# The mode numbers of tools/css/LOG.md.
MODES = [
    ("component_value_list.json", 1),
    ("one_component_value.json", 2),
    ("declaration_list.json", 3),
    ("blocks_contents.json", 4),
    ("one_declaration.json", 5),
    ("one_rule.json", 6),
    ("rule_list.json", 7),
    ("stylesheet.json", 8),
    ("An+B.json", 9),
    ("stylesheet_bytes.json", 10),
]


def piece(raw):
    return struct.pack("<I", len(raw)) + raw


def load():
    """Returns a list of (file, index, mode, input_pieces, expected)."""
    cases = []
    for name, mode in MODES:
        path = os.path.join(DATA, name)
        d = json.load(open(path, encoding="utf-8"))
        for i in range(0, len(d), 2):
            src, expected = d[i], d[i + 1]
            if mode == 10:
                raw = src["css_bytes"].encode("latin-1")
                proto = (src.get("protocol_encoding") or "").encode("utf-8")
                env = (src.get("environment_encoding") or "").encode("utf-8")
                pieces = [raw, proto, env]
            else:
                pieces = [src.encode("utf-8", "surrogatepass"), b"", b""]
            cases.append((name, i // 2, mode, pieces, expected))
    return cases


def equal(a, b):
    """Deep comparison. Numbers by value, booleans strictly."""
    if isinstance(a, bool) or isinstance(b, bool):
        return isinstance(a, bool) and isinstance(b, bool) and a == b
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return a == b
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(equal(x, y) for x, y in zip(a, b))
    return type(a) is type(b) and a == b


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    binary = sys.argv[1]
    json_target = None
    show = 0
    only = None
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
            only = args[i + 1]
            i += 2
        else:
            print("unknown option: %s" % args[i])
            return 2

    cases = [c for c in load() if not only or only in c[0]]
    if not cases:
        print("NO CASES FOUND in %s" % DATA)
        return 1

    payload = b""
    for _, _, mode, pieces, _ in cases:
        payload += struct.pack("<I", mode)
        for p in pieces:
            payload += piece(p)

    p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=600)
    if p.returncode != 0:
        print("THE BINARY ENDED WITH %d" % p.returncode)
        print(p.stderr.decode("utf-8", "replace")[:2000])
        return 1
    lines = p.stdout.decode("utf-8", "surrogatepass").split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    if len(lines) != len(cases):
        print("WRONG NUMBER OF ANSWERS: %d for %d cases" % (len(lines), len(cases)))
        return 1

    per_file = {}
    failures = []
    passed = 0
    for (name, idx, mode, pieces, expected), line in zip(cases, lines):
        total, good = per_file.get(name, (0, 0))
        try:
            got = json.loads(line)
            ok = equal(got, expected)
        except ValueError:
            got = line
            ok = False
        if ok:
            passed += 1
            good += 1
        else:
            failures.append((name, idx, pieces[0], expected, got))
        per_file[name] = (total + 1, good)

    for name, _ in MODES:
        if name in per_file:
            total, good = per_file[name]
            print("%-28s %4d / %4d" % (name, good, total))
    print("TOTAL %d / %d passed" % (passed, len(cases)))

    for name, idx, src, expected, got in failures[:show]:
        print("\n--- %s case %d" % (name, idx))
        print("  input:    %r" % src[:160])
        print("  expected: %s" % json.dumps(expected)[:400])
        print("  got:      %s" % json.dumps(got)[:400] if not isinstance(got, str)
              else "  got:      %s" % got[:400])

    if json_target:
        json.dump({
            "passed": passed,
            "total": len(cases),
            "per_file": {k: {"passed": v[1], "total": v[0]}
                         for k, v in per_file.items()},
        }, open(json_target, "w"), indent=1)
    return 0 if passed == len(cases) else 0


if __name__ == "__main__":
    sys.exit(main())
