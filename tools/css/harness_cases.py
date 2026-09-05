#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Runner for the OWN cases of the CSS path (tools/css/cases/*.txt).

What cssselect2 cannot check is checked here: `:hover`, the cascade over
three origins, the reversal by `!important`, inheritance, computed values
and the error tolerance of the parser on broken CSS. The expectations are
written by hand from the standard, one file per subject.

The format of a case file (sections, each introduced by a `#` line):

    #name    a name for the report
    #html    the document
    #ua      the user agent stylesheet   (origin 0)
    #user    the user stylesheet         (origin 1)
    #author  the author stylesheet       (origin 2)
    #hover   the element index for :hover (optional)
    #match   one line per selector:  <selector> | <index> <index> ...
    #style   one line per element:   <index> | key=value key=value ...
    #spec    one line per selector:  <selector> | a-b-c

The keys of `#style`: display, color, font-size, margin, padding,
border-width, border-style, border-color. `margin` and friends take four
values separated by commas, in the order top, right, bottom, left.

Usage: python3 tools/css/harness_cases.py <binary> [--json file] [--show N]
"""

import glob
import json
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CASES = os.path.join(ROOT, "tools", "css", "cases")

DISPLAY = {0: "inline", 1: "block", 2: "inline-block", 3: "none",
           4: "list-item", 5: "flex", 6: "inline-flex", 7: "grid",
           8: "table", 9: "table-row", 10: "table-cell", 11: "inline-table"}
BSTYLE = {0: "none", 1: "hidden", 2: "dotted", 3: "dashed", 4: "solid",
          5: "double", 6: "groove", 7: "ridge", 8: "inset", 9: "outset"}


def load_case(path):
    case = {"name": os.path.basename(path), "html": "", "ua": "", "user": "",
            "author": "", "hover": None, "match": [], "style": [], "spec": []}
    section = None
    for line in open(path, encoding="utf-8").read().split("\n"):
        if line.startswith("#") and line[1:].split(" ")[0] in (
                "name", "html", "ua", "user", "author", "hover", "match",
                "style", "spec", "end"):
            section = line[1:].strip()
            if section.startswith("name"):
                case["name"] = section[4:].strip() or case["name"]
                section = None
            elif section == "end":
                section = None
            continue
        if section is None:
            continue
        if section == "hover":
            if line.strip():
                case["hover"] = int(line.strip())
        elif section in ("match", "style", "spec"):
            if line.strip() and not line.startswith("//"):
                left, right = line.rsplit("|", 1)
                case[section].append((left.strip(), right.strip()))
        else:
            case[section] += line + "\n"
    return case


def build_job(case, selectors):
    hover = case["hover"] if case["hover"] is not None else 0xFFFFFFFF
    job = struct.pack("<I", hover)
    parts = [case["html"].encode("utf-8"), case["ua"].encode("utf-8"),
             case["user"].encode("utf-8"), case["author"].encode("utf-8"),
             ("\n".join(selectors) + "\n").encode("utf-8")]
    for p in parts:
        job += struct.pack("<I", len(p)) + p
    return job


def parse_answer(text):
    matches, spec, styles = {}, {}, {}
    for line in text.split("\n"):
        f = line.split(" ")
        if line.startswith("M "):
            matches[int(f[1])] = [int(x) for x in f[2:] if x]
        elif line.startswith("P "):
            spec[int(f[1])] = f[2]
        elif line.startswith("S "):
            styles[int(f[1])] = {
                "display": DISPLAY.get(int(f[2]), f[2]),
                "color": f[3],
                "font-size": f[4],
                "margin": ",".join(f[5:9]),
                "padding": ",".join(f[9:13]),
                "border-width": ",".join(f[13:17]),
                "border-style": ",".join(BSTYLE.get(int(x), x) for x in f[17:21]),
                "border-color": ",".join(f[21:25]),
            }
    return matches, spec, styles


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    binary = sys.argv[1]
    json_target = None
    show = 0
    args = sys.argv[2:]
    i = 0
    while i < len(args):
        if args[i] == "--json":
            json_target = args[i + 1]
            i += 2
        elif args[i] == "--show":
            show = int(args[i + 1])
            i += 2
        else:
            print("unknown option: %s" % args[i])
            return 2

    paths = sorted(glob.glob(os.path.join(CASES, "*.txt")))
    if not paths:
        print("NO CASES FOUND in %s" % CASES)
        return 1

    checks = 0
    passed = 0
    failures = []
    per_case = {}
    for path in paths:
        case = load_case(path)
        selectors = [s for s, _ in case["match"]] + [s for s, _ in case["spec"]]
        p = subprocess.run([binary], input=build_job(case, selectors),
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                           timeout=300)
        if p.returncode != 0:
            print("%s: THE BINARY ENDED WITH %d" % (case["name"], p.returncode))
            return 1
        answer = p.stdout.decode("utf-8", "replace")
        matches, spec, styles = parse_answer(answer)
        good = 0
        total = 0
        for si, (sel, want) in enumerate(case["match"]):
            want_list = [int(x) for x in want.split() if x]
            got = matches.get(si, [])
            total += 1
            checks += 1
            if got == want_list:
                good += 1
                passed += 1
            else:
                failures.append((case["name"], "match " + sel, want_list, got))
        base = len(case["match"])
        for si, (sel, want) in enumerate(case["spec"]):
            got = spec.get(base + si, "MISSING")
            total += 1
            checks += 1
            if got == want:
                good += 1
                passed += 1
            else:
                failures.append((case["name"], "spec " + sel, want, got))
        for idx, want in case["style"]:
            got = styles.get(int(idx), {})
            for pair in want.split():
                key, value = pair.split("=", 1)
                total += 1
                checks += 1
                if got.get(key) == value:
                    good += 1
                    passed += 1
                else:
                    failures.append((case["name"], "style %s %s" % (idx, key),
                                     value, got.get(key)))
        per_case[case["name"]] = (good, total)

    for name in sorted(per_case):
        good, total = per_case[name]
        print("%-28s %4d / %4d" % (name, good, total))
    print("TOTAL %d / %d passed" % (passed, checks))
    for name, what, want, got in failures[:show]:
        print("\n--- %s   %s" % (name, what))
        print("  expected: %s" % (str(want)[:200]))
        print("  got:      %s" % (str(got)[:200]))
    if json_target:
        json.dump({"passed": passed, "total": checks,
                   "per_case": {k: {"passed": v[0], "total": v[1]}
                                for k, v in per_case.items()}},
                  open(json_target, "w"), indent=1)
    return 0 if passed == checks else 1


if __name__ == "__main__":
    sys.exit(main())
