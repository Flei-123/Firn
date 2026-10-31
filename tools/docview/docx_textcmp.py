#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import glob
import os
import subprocess
import sys
import xml.etree.ElementTree as ET
import zipfile

W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"


def oracle_words(path):
    with zipfile.ZipFile(path) as z:
        root = ET.fromstring(z.read("word/document.xml"))
    out = []

    def para_text(p):
        parts = []
        for el in p.iter():
            if el.tag == W + "t" and el.text:
                parts.append(el.text)
            elif el.tag in (W + "tab", W + "br", W + "cr"):
                parts.append(" ")
            elif el.tag == W + "noBreakHyphen":
                parts.append("-")
        return "".join(parts)

    for p in root.iter(W + "p"):
        out.append(para_text(p))
    return " ".join(out).split()


def main():
    tool, cmd, corpus = sys.argv[1], sys.argv[2], sys.argv[3]
    bad = 0
    files = sorted(glob.glob(os.path.join(corpus, "*.docx")))
    for path in files:
        want = oracle_words(path)
        try:
            got = subprocess.run([tool, cmd, path], capture_output=True, timeout=120).stdout.decode("utf-8", "replace").split()
        except subprocess.TimeoutExpired:
            got = ["TIMEOUT"]
        if want != got:
            bad += 1
            n = 0
            while n < min(len(want), len(got)) and want[n] == got[n]:
                n += 1
            print("DIFF %s want=%d got=%d first=%d %r vs %r" % (os.path.basename(path), len(want), len(got), n,
                  want[n:n + 3], got[n:n + 3]))
    print("files=%d diff=%d" % (len(files), bad))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
