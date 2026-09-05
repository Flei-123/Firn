#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Test bench for lib/html/entities.fi (character references) -- a workbench.

This script contains NO tokenizer logic. Out of the official html5lib test
data it takes all the cases that can be decided with the character reference
part alone (Data state, no '<' in the input, the expectation consists only of
character tokens), drives them through the test bench written in Firn,
lib/html/entities_probe.fi, and compares.

Cases that this narrow section does not cover are NOT counted here --
the binding total is given by tools/tokenizer/harness.py alone, over
all 6,810 cases. This script is a module proof, not a balance.

Usage:  python3 tools/tokenizer/check_entities.py [binary]
"""

import glob
import json
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TESTDIR = os.path.join(ROOT, "testdata", "html5lib-tokenizer")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import unescape, unescape_token, normalise  # noqa: E402


def cases():
    """All cases that are pure character reference cases in the Data state."""
    out = []
    for path in sorted(glob.glob(os.path.join(TESTDIR, "*.test"))):
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
        lst = data.get("tests")
        if lst is None:
            lst = data.get("xmlViolationTests", [])
        for i, t in enumerate(lst):
            inp = t["input"]
            expected = t["output"]
            if t.get("doubleEscaped"):
                inp = unescape(inp)
                expected = unescape_token(expected)
            states = t.get("initialStates") or ["Data state"]
            if states != ["Data state"]:
                continue
            if "<" in inp or "\0" in inp or "\r" in inp or "&" not in inp:
                continue
            expected = normalise(expected)
            if any(tok[0] != "Character" for tok in expected):
                continue
            out.append((os.path.basename(path), i, t.get("description", ""),
                         inp, expected))
    return out


def main():
    binary = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        ROOT, ".tokenizer-work", "entities_probe")
    if not os.path.exists(binary):
        print("not built: " + binary)
        return 2
    lst = cases()
    raw = bytearray()
    for _, _, _, inp, _ in lst:
        b = inp.encode("utf-8", "surrogatepass")
        # state, flags, len_lasttag, len_input (see PROTOKOLL.md)
        raw += struct.pack("<I", 0) + struct.pack("<I", 0) + struct.pack("<I", 0)
        raw += struct.pack("<I", len(b)) + b
    p = subprocess.run([binary], input=bytes(raw), stdout=subprocess.PIPE)
    lines = p.stdout.decode("ascii", "replace").splitlines()
    if len(lines) != len(lst):
        print("ERROR: %d answers for %d cases" % (len(lines), len(lst)))
        return 1

    good = 0
    bad = []
    for (file, i, desc, inp, expected), line in zip(lst, lines):
        try:
            # Answer line: token stream TAB list of parse errors (PROTOKOLL.md).
            # The test bench only compares the token stream.
            got = normalise(json.loads(line.split("\t")[0]))
        except ValueError:
            got = ["<broken answer>"]
        if got == expected:
            good += 1
        else:
            bad.append((file, i, desc, inp, expected, got))

    print("character references (lib/html/entities.fi), pure Data state cases")
    print("  passed: %d / %d" % (good, len(lst)))
    for file, i, desc, inp, expected, got in bad[:20]:
        print("  FAIL %s #%d %s" % (file, i, desc))
        print("       in  =%r" % inp)
        print("       want=%s" % json.dumps(expected))
        print("       got =%s" % json.dumps(got))
    if bad:
        print("  ... %d failures" % len(bad))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
