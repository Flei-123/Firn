#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/js/fixlen.py -- the two lengths of a text literal, kept in step.

Round 63 wrote down gap 10 of the language: `var m: [u8; 40] = "..."` is an
error if the count is off by one, and the LENGTH that is passed on
afterwards is a SECOND, independent number:

    var a1: [u8; 4] = "keys"
    let k1: bool = try nat(r, c, &a1[0], 4, B_OBJ_KEYS, 1)
                                       ^-- the same 4, by hand

Round 74 adds about six hundred of these pairs. Counting them by hand is
not work, it is a bug generator -- the array length is caught by the
compiler, the SECOND number is not: it silently truncates a name and the
built in ends up installed under the wrong key.

So this program does both, over the files it is given:

  1. `[u8; N] = "..."`  -- N is set to the real length of the literal
     (an escape counts as one octet, `\\u{...}` is not used here),
  2. every call argument of the form `&NAME[0], N` -- N is set to the
     length declared for NAME in the SAME function.

It only ever changes a number that is already there. It never inserts and
never removes anything, so a file that is already right passes through
unchanged; `--check` says so with the exit status instead of writing.

Usage:  python3 tools/js/fixlen.py [--check] FILE...
"""
import re
import sys

DECL = re.compile(r'\b(var|let)\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*\[u8;\s*(\d+)\]\s*=\s*"((?:[^"\\]|\\.)*)"')
LIT = re.compile(r'(\[u8;\s*)(\d+)(\]\s*=\s*")((?:[^"\\]|\\.)*)(")')
USE = re.compile(r'&([A-Za-z_][A-Za-z0-9_]*)\[0\](\s*,\s*)(\d+)')
FN = re.compile(r'^fn\s', re.M)


def declen(s):
    """The octet count of a Firn text literal: an escape is one octet."""
    n = 0
    i = 0
    while i < len(s):
        if s[i] == '\\':
            i += 2
        else:
            i += 1
        n += 1
    return n


def fix(text):
    # 1. the declared array length
    text = LIT.sub(lambda m: m.group(1) + str(declen(m.group(4)))
                   + m.group(3) + m.group(4) + m.group(5), text)
    # 2. the length that is passed on -- per function, so that two
    #    functions may use the same local name for different texts.
    cuts = [m.start() for m in FN.finditer(text)] + [len(text)]
    out = [text[:cuts[0]]] if cuts and cuts[0] > 0 else []
    for a, b in zip(cuts, cuts[1:]):
        body = text[a:b]
        # Every declaration with its position: a function may declare the
        # same local name twice (`var m` in two branches), and then the
        # NEAREST one before the use is the one that counts.
        decls = [(m.start(), m.group(2), declen(m.group(4)))
                 for m in DECL.finditer(body)]

        def rep(m):
            want = None
            for pos, name, ln in decls:
                if name == m.group(1) and pos < m.start():
                    want = ln
            if want is None:
                return m.group(0)
            return "&%s[0]%s%d" % (m.group(1), m.group(2), want)
        out.append(USE.sub(rep, body))
    return "".join(out)


def main():
    args = sys.argv[1:]
    check = False
    if args and args[0] == "--check":
        check = True
        args = args[1:]
    bad = 0
    for p in args:
        src = open(p, encoding="utf-8").read()
        new = fix(src)
        if new == src:
            continue
        bad += 1
        if check:
            print("%s: a text length does not match" % p)
        else:
            open(p, "w", encoding="utf-8").write(new)
            print("%s: corrected" % p)
    if check and bad:
        sys.exit(1)


main()
