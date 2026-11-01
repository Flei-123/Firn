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

        def walk(el):
            if el.tag == W + "txbxContent":
                return
            if el.tag == W + "t" and el.text:
                parts.append(el.text)
            elif el.tag in (W + "tab", W + "br", W + "cr"):
                parts.append(" ")
            elif el.tag == W + "noBreakHyphen":
                parts.append("-")
            for ch in el:
                walk(ch)

        walk(p)
        return "".join(parts)

    for p in root.iter(W + "p"):
        out.append(para_text(p))
    return " ".join(out).split()


HF_EXPECT = [("f08_headerfooter", ["Header", "words", "here"], ["Page", "{n}"])]


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
    for name, head, foot in HF_EXPECT:
        path = os.path.join(corpus, name + ".docx")
        if not os.path.exists(path):
            continue
        out = subprocess.run([tool, "t", path], capture_output=True, timeout=120).stdout.decode("utf-8", "replace")
        words = out.split()
        npages = int(subprocess.run([tool, "i", path], capture_output=True, timeout=120).stdout.split()[0])
        ok = words[:len(head)] == head and npages > 0
        pos = 0
        for i in range(npages):
            want = [w.replace("{n}", str(i + 1)) for w in foot]
            while pos + len(want) <= len(words) and words[pos:pos + len(want)] != want:
                pos += 1
            if pos + len(want) > len(words):
                ok = False
                break
            pos += len(want)
        if not ok:
            bad += 1
            print("HF-DIFF %s %r" % (name, words[:8]))
    print("files=%d diff=%d" % (len(files), bad))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
