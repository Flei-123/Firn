#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Resolve every relative Markdown link in the given files and report the dead
ones.

A link in a README that points at nothing is a broken promise -- the file is
the first thing a stranger reads.  This script takes the files named on the
command line (default: README.md, docs/*.md and the .md files in the root),
pulls out the `[text](target)` links, throws away the ones that leave the
repository (`http://`, `https://`, `mailto:`) and the pure anchors
(`#section`), resolves the rest relative to the DIRECTORY OF THE FILE THE LINK
STANDS IN, and checks that the path exists.

    python3 tools/mdlinks/check.py                  # README.md + docs/*.md + *.md
    python3 tools/mdlinks/check.py README.md        # only this one

Exit code 0 = every link resolves, 1 = at least one does not.
"""

import os
import re
import sys

# `[text](target)` -- the target must not contain a bracket or whitespace that
# would make it a reference-style link or a title.
LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")

# A link inside a fenced code block is sample text, not a link.
FENCE = re.compile(r"^\s*```")

# ... and neither is one inside an inline code span: `gcvec_append[Node](...)`
# is a piece of Firn, not a reference to a file.
CODE_SPAN = re.compile(r"`[^`]*`")


def links_of(path):
    """Yield (line number, target) for every link outside a code fence."""
    inside_fence = False
    with open(path, encoding="utf-8") as handle:
        for number, line in enumerate(handle, 1):
            if FENCE.match(line):
                inside_fence = not inside_fence
                continue
            if inside_fence:
                continue
            for target in LINK.findall(CODE_SPAN.sub("``", line)):
                yield number, target


def is_external(target):
    return (
        target.startswith("http://")
        or target.startswith("https://")
        or target.startswith("mailto:")
        or target.startswith("#")
    )


def main(argv):
    root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
    os.chdir(root)

    files = argv[1:]
    if not files:
        files = ["README.md"]
        for name in sorted(os.listdir(".")):
            if name.endswith(".md") and name not in files:
                files.append(name)
        for name in sorted(os.listdir("docs")):
            if name.endswith(".md"):
                files.append(os.path.join("docs", name))

    checked = 0
    external = 0
    dead = []
    for path in files:
        if not os.path.exists(path):
            dead.append((path, 0, "<the file itself does not exist>"))
            continue
        base = os.path.dirname(path)
        for number, target in links_of(path):
            if is_external(target):
                external += 1
                continue
            # strip an anchor: docs/FIR.md#instructions -> docs/FIR.md
            bare = target.split("#", 1)[0]
            if not bare:
                continue
            checked += 1
            resolved = os.path.normpath(os.path.join(base, bare))
            if not os.path.exists(resolved):
                dead.append((path, number, target))

    print("files        : %d" % len(files))
    print("links local  : %d" % checked)
    print("links extern : %d (not fetched)" % external)
    if dead:
        print("DEAD         : %d" % len(dead))
        for path, number, target in dead:
            print("   %s:%d  ->  %s" % (path, number, target))
        return 1
    print("DEAD         : 0")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
