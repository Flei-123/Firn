#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/bench90/icount.py -- INSTRUCTIONS, not seconds.

The machine this project is measured on is shared, and a wall clock median
of seven runs still moves by ten percent between two passes. That is enough
to hide a real five percent and to invent one that is not there.

`valgrind --tool=callgrind` counts the instructions the program really
executed. It is deterministic to the last digit, so a difference in this
number is a difference in the code, not in the neighbours. It says nothing
about cache misses or branch mispredictions -- for those the wall clock in
`bench.py` stays the measurement -- but for "did this change make the loop
shorter" it is the honest answer.

    python3 tools/bench90/icount.py [--firnc <path>] [--tag <name>]
"""
import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
WORK = os.path.join(HERE, ".icount")

BENCHES = ["fib", "sieve", "matmul", "bytecount", "bubblesort", "statemachine",
           "bitmap", "xxhash", "jsonscan", "memstride", "branchy"]
# The big ones are cut down: callgrind is about 50x slower than the machine.
SMALL = {"xxhash", "memstride", "branchy", "bytecount", "jsonscan"}


def icount(binary):
    out = os.path.join(WORK, "cg.out")
    r = subprocess.run(
        ["valgrind", "--tool=callgrind", "--callgrind-out-file=" + out, binary],
        capture_output=True, text=True)
    m = re.search(r"refs:\s+([\d,]+)", r.stderr)
    if not m:
        print(r.stderr[-800:])
        sys.exit(1)
    return int(m.group(1).replace(",", ""))


def main():
    firnc = os.path.join(ROOT, "compiler/target/release/firnc")
    tag = "cur"
    args = sys.argv[1:]
    while args:
        a = args.pop(0)
        if a == "--firnc":
            firnc = args.pop(0)
        elif a == "--tag":
            tag = args.pop(0)
        elif a == "--only":
            global BENCHES
            BENCHES = args.pop(0).split(",")
    os.makedirs(WORK, exist_ok=True)
    rows = {}
    for name in BENCHES:
        fi = os.path.join(ROOT, "bench/firn", name + ".fi")
        rs = os.path.join(ROOT, "bench/rust", name + ".rs")
        if not os.path.exists(fi):
            continue
        r = {}
        for lvl, flag in (("fast", "--opt-level=release-fast"), ("safe", "--opt-level=release-safe")):
            b = os.path.join(WORK, "%s.%s.%s" % (name, tag, lvl))
            p = subprocess.run([firnc, flag, "-o", b, fi], capture_output=True, text=True)
            if p.returncode != 0:
                print("compile failed %s %s: %s" % (name, lvl, p.stderr[:300]))
                sys.exit(1)
            r["firn_" + lvl] = icount(b)
        if os.path.exists(rs):
            for lvl, chk in (("fast", "no"), ("safe", "yes")):
                b = os.path.join(WORK, "%s.rust.%s" % (name, lvl))
                if not os.path.exists(b):
                    subprocess.run(["rustc", "-O", "-C", "overflow-checks=" + chk, "-o", b, rs],
                                   capture_output=True, text=True)
                r["rust_" + lvl] = icount(b)
        rows[name] = r
        print("%-14s firn fast %13d  safe %13d | rust fast %13d safe %13d"
              % (name, r.get("firn_fast", 0), r.get("firn_safe", 0),
                 r.get("rust_fast", 0), r.get("rust_safe", 0)), flush=True)
    out = os.path.join(WORK, "icount-%s.json" % tag)
    with open(out, "w") as fh:
        json.dump(rows, fh, indent=1)
    print("written: %s" % out)


if __name__ == "__main__":
    main()
