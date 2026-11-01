#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import collections
import glob
import os
import re
import subprocess
import sys


def words(text):
    return collections.Counter(re.findall(r"\w+", text.lower()))


def main():
    tool, corpus = sys.argv[1], sys.argv[2]
    total = hit = files = 0
    for path in sorted(glob.glob(os.path.join(corpus, "*.pdf"))):
        ref = subprocess.run(["pdftotext", "-f", "1", "-l", "1", "-enc", "UTF-8", path, "-"],
                             capture_output=True, timeout=60).stdout.decode("utf-8", "replace")
        rw = words(ref)
        if sum(rw.values()) < 20:
            continue
        try:
            got = subprocess.run([tool, "t", path, "0"], capture_output=True, timeout=60).stdout.decode("utf-8", "replace")
        except subprocess.TimeoutExpired:
            got = ""
        gw = words(got)
        common = sum((rw & gw).values())
        n = sum(rw.values())
        files += 1
        total += n
        hit += common
        print("%-60s ref=%5d match=%5.1f%%" % (os.path.basename(path)[:60], n, 100.0 * common / n))
    print("files %d words %d match %.2f%%" % (files, total, 100.0 * hit / max(total, 1)))


main()
