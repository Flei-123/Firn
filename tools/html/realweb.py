#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Robustness probe of the tree construction on REAL pages.

The eight pages in `testdata/realweb/` are unchanged copies of real
web pages (0.03 to 1.0 MB). What is checked here is only what can be
checked without a foreign library:

  * the binary runs through and reports no `#KAPUTT` abort,
  * a non-empty tree comes out,
  * the output is reproducible (run.sh compares the three build stages
    byte for byte against each other).

The COMPARISON with an independent implementation is not here but in
tools/html/oracle.py (which needs html5lib and therefore the network).

Usage:  python3 tools/html/realweb.py <binary>
"""

import glob
import hashlib
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CORPUS = os.path.join(ROOT, "testdata", "realweb")


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    binary = sys.argv[1]
    paths = sorted(glob.glob(os.path.join(CORPUS, "*.html")))
    if not paths:
        print("NO CORPUS in %s" % CORPUS)
        return 1
    data = []
    for p in paths:
        with open(p, encoding="utf-8", errors="replace") as fh:
            data.append(fh.read())
    payload = b""
    for d in data:
        raw = d.encode("utf-8", "surrogatepass")
        payload += struct.pack("<I", len(raw)) + raw

    p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=900)
    if p.returncode != 0:
        print("THE BINARY ENDED WITH %d" % p.returncode)
        return 1
    parts = p.stdout.decode("utf-8", "surrogatepass").split("#ENDE\n")
    if parts and parts[-1] == "":
        parts.pop()
    if len(parts) != len(paths):
        print("WRONG NUMBER OF ANSWERS: %d for %d pages" % (len(parts), len(paths)))
        return 1
    bad = 0
    for path, source, answer in zip(paths, data, parts):
        lines = answer.count("\n")
        broken = "#KAPUTT" in answer
        digest = hashlib.sha256(answer.encode("utf-8", "surrogatepass")).hexdigest()[:16]
        if broken or lines < 10:
            bad += 1
        print("%-26s %9d B ->%8d lines  %s  %s"
              % (os.path.basename(path), len(source), lines, digest,
                 "BROKEN" if broken else "ok"))
    print("%d pages, %d objected to" % (len(paths), bad))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
