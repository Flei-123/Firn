#!/usr/bin/env python3
"""tools/dwarf/vars_check.py -- do the variables a debugger shows AGREE?

Round 96. Reads a `gdb` batch output and holds every `name = value` line in
it against a table of expectations. Three numbers come out:

    checked=<n> wrong=<n> missing=<n>

`checked`  a variable that was there and had the expected value
`wrong`    a variable that was there and had ANOTHER value -- the one number
           that must stay zero. A wrong value in a debugger is worse than
           none (docs/DEBUGGER.md), so this is the failure, not `missing`.
`missing`  a variable the expectation names and gdb did not show, or showed
           as `<optimized out>`. Counted, never hidden: it is the price of
           the optimization and the round reports it as a number.

Usage:  python3 tools/dwarf/vars_check.py <gdb-output> name=value [name=value ...]
"""
import re
import sys

LINE = re.compile(r"^\s*([A-Za-z_][A-Za-z0-9_]*) = (.*)$")


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    seen = {}
    for line in open(argv[1], encoding="utf-8", errors="replace"):
        m = LINE.match(line.rstrip("\n"))
        if m:
            # The LAST occurrence wins: the later breakpoint is the one the
            # expectation is written for.
            seen[m.group(1)] = m.group(2).strip()
    checked = 0
    wrong = 0
    missing = 0
    for want in argv[2:]:
        name, _, value = want.partition("=")
        got = seen.get(name)
        if got is None or "optimized out" in got or got == "<error>":
            missing += 1
            print("  missing %s (expected %s, gdb said %s)" % (name, value, got))
            continue
        if got != value:
            wrong += 1
            print("  WRONG   %s: gdb says %s, the program computes %s" % (name, got, value))
            continue
        checked += 1
        print("  ok      %s = %s" % (name, got))
    print("checked=%d wrong=%d missing=%d" % (checked, wrong, missing))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
