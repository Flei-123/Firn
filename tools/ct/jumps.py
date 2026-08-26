#!/usr/bin/env python3
"""tools/ct/jumps.py -- count the jumps of an assembly text, per function.

Reads a `.s` file produced by `firnc --emit=asm` and prints one line per
function:

    <name> lines=<n> jcc=<n> control=<n> cmov=<n>

`jcc`     every conditional jump (`j<cc>`, `jmp` is not one)
`control` those of them that are CONTROL FLOW of the program itself
`cmov`    conditional moves

The split matters and it is the whole reason this script exists. At the
`dev` build level an array access carries a bounds check and an arithmetic
operation carries an overflow check (SPEC 13, `L9`); both are a conditional
jump, both jump to a label of the checking machinery
(`.Lchkidx…`, `.Lchk…`, `.Lpanic…`, `.Lsat…`), and both are decided by
PUBLIC data -- an index that stands in the source text, a length that stands
in the type. They always go the same way and they betray nothing.

A conditional jump to any OTHER label is control flow of the program: an
`if`, a `while`, a `&&`. That is the number that has to be zero inside a
`#[constant_time]` function, and it is the number `control` reports.

Usage:  python3 tools/ct/jumps.py file.s [function ...]
"""
import re
import sys

# A label the checking machinery jumps to. Everything else is control flow.
CHECK_LABEL = re.compile(r"^\.L(chkidx|chk|panic|sat|ovf)")
JCC = re.compile(r"^\s*(j(?!mp\b)[a-z]+)\s+(\S+)")
FUNC = re.compile(r"^([A-Za-z_][A-Za-z0-9_.$]*):$")


def read(path):
    out = {}
    cur = None
    for line in open(path, encoding="utf-8", errors="replace"):
        line = line.rstrip("\n")
        m = FUNC.match(line)
        if m:
            cur = m.group(1)
            out[cur] = []
            continue
        if cur is not None:
            out[cur].append(line)
    return out


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    bodies = read(argv[1])
    wanted = argv[2:]
    rc = 0
    for name, body in bodies.items():
        short = name.split(".")[-1]
        if wanted and short not in wanted and name not in wanted:
            continue
        jcc = 0
        control = 0
        cmov = 0
        for line in body:
            m = JCC.match(line)
            if m:
                jcc += 1
                if not CHECK_LABEL.match(m.group(2)):
                    control += 1
            if "cmov" in line:
                cmov += 1
        print("%s lines=%d jcc=%d control=%d cmov=%d" % (short, len(body), jcc, control, cmov))
    if wanted:
        have = {n.split(".")[-1] for n in bodies}
        for w in wanted:
            if w not in have:
                print("MISSING %s" % w)
                rc = 1
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv))
