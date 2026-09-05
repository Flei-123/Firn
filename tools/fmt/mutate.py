#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/fmt/mutate.py -- the random test for firnfmt.

It scrambles the BLANKS of a Firn source text without touching a single
token: indentation grows or vanishes, blanks between tokens grow or vanish,
lines get blanks at the end, empty lines get more empty lines. The token
stream stays untouched, so the program stays the same program.

The claim that tools/fmt/run.sh checks with it:

    firnfmt(scrambled)  ==  firnfmt(original)

That is more than idempotence. Idempotence only says that a second run
changes nothing; this says that the shape depends ONLY on the tokens and on
the line structure -- exactly what "canonical" means.

Where the scrambling has to hold back, and why:

  * inside text literals and comments nothing is touched -- those are
    verbatim pieces of the output.
  * two blanks are only removed where the two neighbouring characters do
    not grow together into something new: `a b` must not become `ab`, `:` `:`
    must not become `::`, `/` `/` must not become a comment.
  * EMPTY LINES are only added where there already is one (or right after a
    `{`, where the formatter throws them away anyway). An empty line is a
    part of the shape that firnfmt deliberately keeps -- one that is added
    out of nowhere would be a real difference, not noise.

Usage:  mutate.py <in.fi> <out.fi> [seed]
"""
import random
import sys

IDENT = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_")
DIGIT = set("0123456789")
# Character pairs that would become ONE token, or a comment.
PAIRS = {"::", "=>", "->", "==", "!=", "<=", ">=", "<<", ">>", "&&", "||",
         "..", "//", "/*", "*/", "+=", "-="}


def states(src):
    """Per character: True = code, False = text literal or comment.

    Additionally the state at the START of every line, so that empty lines
    are only inserted outside of block comments.
    """
    n = len(src)
    code = [False] * n
    i = 0
    while i < n:
        c = src[i]
        if c == '/' and i + 1 < n and src[i + 1] == '/':
            while i < n and src[i] != '\n':
                i += 1
            continue
        if c == '/' and i + 1 < n and src[i + 1] == '*':
            depth = 1
            i += 2
            while i < n and depth > 0:
                if src.startswith("/*", i):
                    depth += 1
                    i += 2
                elif src.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue
        if c == '"' or (c in "buf" and i + 1 < n and src[i + 1] == '"'):
            if c != '"':
                i += 1
            i += 1
            while i < n:
                if src[i] == '\\':
                    i += 2
                    continue
                if src[i] == '"':
                    i += 1
                    break
                if src[i] == '\n':
                    break
                i += 1
            continue
        code[i] = True
        i += 1
    return code


def runs_of_blanks(src, code):
    """Every run of blanks INSIDE a line that lies in code.

    A run is (start, end). The line break itself is never part of it -- the
    scrambling must never join two lines.
    """
    out = []
    n = len(src)
    i = 0
    while i < n:
        if src[i] in " \t" and (i == 0 or src[i - 1] != '\\'):
            j = i
            while j < n and src[j] in " \t":
                j += 1
            # in code? the character after the run decides (the run itself
            # carries no state)
            after_ok = j >= n or src[j] == '\n' or code[j]
            before_ok = i == 0 or src[i - 1] == '\n' or code[i - 1]
            if after_ok and before_ok:
                out.append((i, j))
            i = j
            continue
        i += 1
    return out


def removable(src, a, b):
    """May the run of blanks from `a` to `b` disappear completely?"""
    if a == 0 or src[a - 1] == '\n':
        return True                      # indentation always may
    if b >= len(src) or src[b] == '\n':
        return True                      # blanks at the end of a line too
    x, y = src[a - 1], src[b]
    if x in IDENT and y in IDENT:
        return False
    if x in IDENT and y == '"':
        return False                     # would make b"..." out of b "..."
    if x in DIGIT and y == '.':
        return False
    if x == '.' and y in DIGIT:
        return False
    if x + y in PAIRS:
        return False
    return True


def scramble(src, rnd):
    code = states(src)
    runs = runs_of_blanks(src, code)
    pieces = []
    last = 0
    for (a, b) in runs:
        pieces.append(src[last:a])
        r = rnd.random()
        if r < 0.30:
            pieces.append(" " * rnd.randint(1, 5))
        elif r < 0.45:
            pieces.append("\t")
        elif r < 0.60 and removable(src, a, b):
            pieces.append("")
        else:
            pieces.append(src[a:b])
        last = b
    pieces.append(src[last:])
    out = "".join(pieces)

    # blanks at the end of the line
    lines = out.split("\n")
    for i in range(len(lines)):
        if rnd.random() < 0.15 and lines[i].strip() != "":
            lines[i] = lines[i] + " " * rnd.randint(1, 3)
    out = "\n".join(lines)

    # more empty lines, only where the shape allows it
    lines = out.split("\n")
    code2 = states(out)
    starts = []
    pos = 0
    for ln in lines:
        starts.append(pos)
        pos += len(ln) + 1
    fresh = []
    for i, ln in enumerate(lines):
        fresh.append(ln)
        in_code = i + 1 >= len(lines) or starts[i + 1] >= len(code2) or \
            code2[starts[i + 1]] or lines[i + 1].strip() == ""
        empty_follows = i + 1 < len(lines) and lines[i + 1].strip() == ""
        after_brace = ln.rstrip().endswith("{") and \
            (starts[i] + len(ln.rstrip()) - 1 < len(code2) and
             code2[starts[i] + len(ln.rstrip()) - 1])
        if in_code and (empty_follows or after_brace) and rnd.random() < 0.4:
            for _ in range(rnd.randint(1, 2)):
                fresh.append("")
    return "\n".join(fresh)


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    seed = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    src = open(sys.argv[1], encoding="utf-8", errors="surrogateescape").read()
    out = scramble(src, random.Random(seed))
    open(sys.argv[2], "w", encoding="utf-8", errors="surrogateescape").write(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
