#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Cross-check of the SELECTOR ENGINE from lib/css/sel.fi against cssselect2.

WHAT IS COMPARED, and what is deliberately not: the matcher, ON THE SAME
TREE. The binary written in Firn parses the page, prints its element table
(one line per element with parent, tag and attributes) and, per selector,
the indices of the elements it matched. This script rebuilds exactly that
tree in Python and lets cssselect2 (an independent implementation, from the
authors of WeasyPrint) match the same selectors on it. Compared are the
SETS OF INDICES and the SPECIFICITY.

So the comparison is free of a second HTML parser: a deviation in the tree
construction cannot pretend to be a deviation in the selector engine. The
tree construction has its own proof (tools/html/run.sh).

Selectors that cssselect2 cannot parse (`:hover`, for example) are counted
separately and reported -- NOT quietly dropped.

Usage: python3 tools/css/harness_select.py <binary> [--json file] [--show N]
                                           [--pages N]
"""

import glob
import json
import os
import struct
import subprocess
import sys
from xml.etree import ElementTree as ET

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CORPUS = os.path.join(ROOT, "testdata", "realweb")
CASES = os.path.join(ROOT, "tools", "css", "cases")
SELECTORS = os.path.join(ROOT, "tools", "css", "selectors.txt")

try:
    import cssselect2
except ImportError:
    cssselect2 = None


def unescape(s):
    out = bytearray()
    i = 0
    raw = s.encode("ascii")
    while i < len(raw):
        if raw[i] == 0x25:
            out.append(int(raw[i + 1:i + 3], 16))
            i += 3
        else:
            out.append(raw[i])
            i += 1
    return out.decode("utf-8", "replace")


def load_selectors():
    out = []
    for line in open(SELECTORS, encoding="utf-8"):
        line = line.rstrip("\n")
        if line.strip() and not line.startswith("#"):
            out.append(line)
    return out


def read_document(path):
    """A page, or the #html section of a case file."""
    raw = open(path, "rb").read()
    if not path.endswith(".txt"):
        return raw
    out = []
    section = None
    for line in raw.decode("utf-8").split("\n"):
        if line.startswith("#") and line[1:].split(" ")[0] in (
                "name", "html", "ua", "user", "author", "hover", "match",
                "style", "spec", "end"):
            section = line[1:].strip()
            continue
        if section == "html":
            out.append(line)
    return ("\n".join(out)).encode("utf-8")


def build_job(html, selectors):
    job = struct.pack("<I", 0xFFFFFFFF)
    parts = [html, b"", b"", b"", ("\n".join(selectors) + "\n").encode("utf-8")]
    for p in parts:
        job += struct.pack("<I", len(p)) + p
    return job


def parse_answer(text):
    """Returns (elements, matches, specificity)."""
    elements = []
    matches = {}
    spec = {}
    for line in text.split("\n"):
        if line.startswith("E "):
            f = line.split(" ")
            idx = int(f[1])
            parent = int(f[2])
            tag = unescape(f[3])
            attrs = {}
            for a in f[4:]:
                if "=" in a:
                    k, v = a.split("=", 1)
                    attrs[unescape(k)] = unescape(v)
                else:
                    attrs[unescape(a)] = ""
            elements.append((idx, parent, tag, attrs))
        elif line.startswith("M "):
            f = line.split(" ")
            matches[int(f[1])] = [int(x) for x in f[2:] if x]
        elif line.startswith("P "):
            f = line.split(" ")
            spec[int(f[1])] = f[2]
    return elements, matches, spec


def build_tree(elements):
    nodes = []
    root = None
    for idx, parent, tag, attrs in elements:
        e = ET.Element(tag, attrs)
        nodes.append(e)
        if parent < 0:
            if root is None:
                root = e
        else:
            nodes[parent].append(e)
    return root, nodes


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    binary = sys.argv[1]
    json_target = None
    show = 0
    max_pages = 0
    args = sys.argv[2:]
    i = 0
    while i < len(args):
        if args[i] == "--json":
            json_target = args[i + 1]
            i += 2
        elif args[i] == "--show":
            show = int(args[i + 1])
            i += 2
        elif args[i] == "--pages":
            max_pages = int(args[i + 1])
            i += 2
        else:
            print("unknown option: %s" % args[i])
            return 2

    if cssselect2 is None:
        print("cssselect2 IS MISSING -- no cross-check possible.")
        print("install:  pip3 install cssselect2")
        return 1

    selectors = load_selectors()
    pages = sorted(glob.glob(os.path.join(CORPUS, "*.html")))
    # The documents of the own cases run through the cross-check as well:
    # they contain structures the real pages do not have (lists with
    # exactly four items for nth-child, tables without tbody).
    pages += sorted(glob.glob(os.path.join(CASES, "*.txt")))
    if max_pages:
        pages = pages[:max_pages]

    compiled = {}
    unsupported = []
    for i, s in enumerate(selectors):
        try:
            compiled[i] = cssselect2.compile_selector_list(s)
        except Exception as exc:
            unsupported.append((s, str(exc)))

    checks = 0
    equal = 0
    spec_checks = 0
    spec_equal = 0
    failures = []
    total_elements = 0
    for path in pages:
        html = read_document(path)
        p = subprocess.run([binary], input=build_job(html, selectors),
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                           timeout=900)
        if p.returncode != 0:
            print("%s: THE BINARY ENDED WITH %d" % (path, p.returncode))
            return 1
        answer = p.stdout.decode("utf-8", "replace")
        elements, matches, spec = parse_answer(answer)
        total_elements += len(elements)
        root, nodes = build_tree(elements)
        if root is None:
            print("%s: no elements" % path)
            return 1
        wrapper = cssselect2.ElementWrapper.from_xml_root(root)
        # The wrappers in document order -- the same order the binary uses.
        order = {}
        for w in wrapper.iter_subtree():
            order[id(w.etree_element)] = len(order)
        by_index = {}
        for w in wrapper.iter_subtree():
            by_index[order[id(w.etree_element)]] = w

        for si, sel in enumerate(selectors):
            if si not in compiled:
                continue
            want = []
            for idx in sorted(by_index):
                w = by_index[idx]
                if any(c.test(w) for c in compiled[si]):
                    want.append(idx)
            got = matches.get(si, [])
            checks += 1
            if got == want:
                equal += 1
            else:
                failures.append((os.path.basename(path), sel, want, got))
            # the specificity of each selector of the group
            want_spec = ",".join("%d-%d-%d" % c.specificity for c in compiled[si])
            got_spec = spec.get(si, "invalid")
            spec_checks += 1
            if want_spec == got_spec:
                spec_equal += 1
            else:
                failures.append((os.path.basename(path), sel + "  [specificity]",
                                 want_spec, got_spec))

    print("pages            : %d (%d elements in all)" % (len(pages), total_elements))
    print("selectors        : %d, of them %d not parsable by cssselect2"
          % (len(selectors), len(unsupported)))
    for s, why in unsupported:
        print("   not cross-checked: %-24s (%s)" % (s, why.split("\n")[0][:60]))
    print("match comparisons: %d / %d equal" % (equal, checks))
    print("specificity      : %d / %d equal" % (spec_equal, spec_checks))

    for name, sel, want, got in failures[:show]:
        print("\n--- %s   selector %s" % (name, sel))
        print("  cssselect2: %s" % (str(want)[:300]))
        print("  firn      : %s" % (str(got)[:300]))

    if json_target:
        json.dump({
            "pages": len(pages),
            "selectors": len(selectors),
            "unsupported": len(unsupported),
            "checks": checks, "equal": equal,
            "spec_checks": spec_checks, "spec_equal": spec_equal,
        }, open(json_target, "w"), indent=1)
    return 0 if (equal == checks and spec_equal == spec_checks) else 1


if __name__ == "__main__":
    sys.exit(main())
