#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Builds an input corpus for the throughput measurement.

There are TWO corpora, because a single one is misleading:

  --source html5lib  (the default)
      The inputs of the html5lib cases (only those starting in the Data
      state, without the doubleEscaped special cases), concatenated many
      times over.

      NOTE, named honestly: this corpus is DELIBERATELY PATHOLOGICAL
      and measures the worst case, not everyday use. The html5lib suite
      consists almost only of edge cases -- broken tags, truncated
      character references, null bytes, unpaired surrogates, doctype
      rubbish, thousands of very short inputs. The share of state changes
      per byte is a multiple of that in real HTML, and long runs of text --
      the case every tokenizer does quickly -- are practically absent.
      An MB/s value on this corpus is therefore NO statement about how
      quickly real pages are processed. It is reported all the same,
      because it measures exactly the work the test suite checks.

  --source realweb
      The eight stored real pages from testdata/realweb/ (Wikipedia, the
      WHATWG standard, W3C, rustdoc, Hacker News), unchanged as delivered.
      That is the everyday case.

Usage:  python3 tools/tokenizer/korpus.py [--source html5lib|realweb] [--mb N]
"""
import glob
import json
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TARGET_MB = float(os.environ.get("KORPUS_MB", "4"))


def corpus_html5lib():
    """Edge case corpus: all html5lib inputs, doubled up to TARGET_MB."""
    pieces = []
    for path in sorted(glob.glob(os.path.join(ROOT, "testdata", "html5lib-tokenizer", "*.test"))):
        d = json.load(open(path, encoding="utf-8"))
        lst = d.get("tests") or d.get("xmlViolationTests", [])
        for t in lst:
            if t.get("doubleEscaped") or t.get("initialStates"):
                continue
            pieces.append(t["input"])
    text = "\n".join(pieces)
    while len(text.encode("utf-8", "surrogatepass")) < TARGET_MB * 1048576:
        text += "\n" + text
    return text.encode("utf-8", "surrogatepass")


def corpus_realweb():
    """Real pages from testdata/realweb/, concatenated unchanged."""
    dirname = os.path.join(ROOT, "testdata", "realweb")
    files = sorted(glob.glob(os.path.join(dirname, "*.html")))
    if not files:
        sys.exit("korpus.py: no pages found in testdata/realweb/")
    raw = b"\n".join(open(p, "rb").read() for p in files)
    # The eight stored pages already give > 4 MB; it is only doubled
    # in case somebody removes pages -- better measurably large than silently too
    # small.
    while len(raw) < TARGET_MB * 1048576:
        raw += b"\n" + raw
    return raw


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    html_path, job_path = sys.argv[1], sys.argv[2]
    source = "html5lib"
    if "--source" in sys.argv:
        source = sys.argv[sys.argv.index("--source") + 1]
    if source == "html5lib":
        raw = corpus_html5lib()
    elif source == "realweb":
        raw = corpus_realweb()
    else:
        sys.exit("korpus.py: unknown source '%s' (html5lib|realweb)" % source)

    open(html_path, "wb").write(raw)
    with open(job_path, "wb") as fh:
        # state, flags, len_lasttag, len_input (see PROTOKOLL.md)
        fh.write(struct.pack("<I", 0) + struct.pack("<I", 0) + struct.pack("<I", 0))
        fh.write(struct.pack("<I", len(raw)) + raw)
    print("   corpus (%s): %.2f MB" % (source, len(raw) / 1048576))


if __name__ == "__main__":
    main()
