#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/liveb4/url_check.py -- the URL resolver against somebody else's.

`urllib.parse.urljoin` implements RFC 3986 section 5 and shares no line of
code with `lib/net/url.fi`. Every case below is fed to both and the answers
are compared as text. The cases are not invented: they are the reference
resolution examples of RFC 3986, 5.4.1 and 5.4.2 -- including the abnormal
ones, which is where a hand-written resolver goes wrong.

The counter-check is in the list: `urljoin` and this resolver differ on
purpose in exactly one place -- a `..` that would climb above the root is
kept at the root by both, but `urljoin` keeps a scheme-relative reference
to an unknown scheme and this one refuses it. Those cases are marked and
counted separately rather than quietly left out.
"""
import subprocess
import sys
from urllib.parse import urljoin

BASE = "http://a/b/c/d;p?q"

CASES = [
    # RFC 3986, 5.4.1 -- normal examples
    "g:h", "g", "./g", "g/", "/g", "//g", "?y", "g?y", "#s", "g#s",
    "g?y#s", ";x", "g;x", "g;x?y#s", "", ".", "./", "..", "../", "../g",
    "../..", "../../", "../../g",
    # 5.4.2 -- abnormal examples
    "../../../g", "../../../../g", "/./g", "/../g", "g.", ".g", "g..",
    "..g", "./../g", "./g/.", "g/./h", "g/../h", "g;x=1/./y",
    "g;x=1/../y", "g?y/./x", "g?y/../x", "g#s/./x", "g#s/../x",
    # a few of our own, all of them things a page really contains
    "http://other/x", "//other/x", "/a/b/../c", "sub/page.html",
    "?only=query", "#only-fragment", "  spaced  ", "with\ttab",
]

# `g:h` is an absolute URL with a scheme this client does not fetch, and
# `mailto:` and friends land in the same place: refused, not guessed at.
UNSUPPORTED_SCHEME = {"g:h"}

# Two places where a BROWSER is deliberately not RFC 3986, and `urljoin`
# is. They are listed here with the answer this resolver gives, so that a
# change to either side shows up as a failure instead of disappearing:
#
#   * an empty path becomes "/" (WHATWG URL 4.4, "path start state").
#     `http://g` and `http://g/` are the same request, and every browser
#     sends the second.
#   * leading and trailing spaces and C0 controls are STRIPPED
#     (WHATWG URL 4.4, step 1). `<a href=" x ">` is a link to `x`.
WHATWG = {
    "//g": "http://g/",
    "  spaced  ": "http://a/b/c/spaced",
    "with\ttab": "http://a/b/c/withtab",
}


def main():
    binary = sys.argv[1]
    jobs = []
    for c in CASES:
        jobs.append(BASE + "\t" + c)
    p = subprocess.run([binary], input=("\n".join(jobs) + "\n").encode(),
                       stdout=subprocess.PIPE, timeout=60)
    got = p.stdout.decode().splitlines()
    if len(got) != len(CASES):
        print("   url: %d answers for %d cases" % (len(got), len(CASES)))
        return 1
    same = 0
    differ = []
    refused = 0
    for c, g in zip(CASES, got):
        want = urljoin(BASE, c)
        # The fragment never goes on the wire, so it is not in the answer.
        want = want.split("#")[0]
        if c in UNSUPPORTED_SCHEME:
            if g == "ERR":
                refused += 1
            else:
                differ.append((c, want, g))
            continue
        if c in WHATWG:
            if g == WHATWG[c]:
                same += 1
            else:
                differ.append((c, WHATWG[c] + " (WHATWG)", g))
            continue
        if g == want:
            same += 1
        else:
            differ.append((c, want, g))
    print("   url resolution   %d / %d agree (%d of them WHATWG rather "
          "than RFC 3986), %d refused by name"
          % (same, len(CASES) - len(UNSUPPORTED_SCHEME), len(WHATWG),
             refused))
    for c, w, g in differ[:8]:
        print("      %-18s want %-30s got %s" % (repr(c), w, g))
    if differ:
        print("URL FAIL: %d of %d differ" % (len(differ), len(CASES)))
        return 1
    print("URL OK: %d cases, 0 differences against urllib" % len(CASES))
    return 0


if __name__ == "__main__":
    sys.exit(main())
