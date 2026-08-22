#!/usr/bin/env python3
"""tools/bench82/ra_report.py -- how good IS the register allocation? (round 82)

Reads the lines that `FIRN_RA_STATS=1` writes to stderr and adds them up:

    RA <name> values=N regs=N imm=N frameaddr=N spilled=N cells=N insts=N
    RA-BASE <name> reason=<why> insts=N

The first form is a function the linear scan of round 43 really allocated;
the second is one that fell back to the base path of `codegen_x86.rs`, where
every value lives in a frame slot.

WHAT THE NUMBERS MEAN, and this matters for reading them:

  * `values` counts FIR values, not variables. A `%n` that never leaves a
    register and one that is immediately consumed both count once.
  * `imm` are constants that stand as an immediate operand at EVERY use site.
    They cost neither a register nor a slot, so they are NOT spills.
  * `frameaddr` are `alloca` values whose address is folded into the operand.
    Likewise no spill.
  * `spilled` is what is left: values that really live in memory and are
    reloaded at every use. That is the number this report is about.

Usage:

    FIRN_RA_STATS=1 firnc -o /tmp/x program.fi 2>&1 | python3 tools/bench82/ra_report.py
    ... | python3 tools/bench82/ra_report.py --hot 20      # the 20 biggest
"""
import re
import sys

RA = re.compile(
    r"^RA (\S+) values=(\d+) regs=(\d+) imm=(\d+) frameaddr=(\d+) "
    r"spilled=(\d+) cells=(\d+) insts=(\d+)"
)
BASE = re.compile(r"^RA-BASE (\S+) reason=(.+) insts=(\d+)")


def main() -> int:
    hot = 0
    if "--hot" in sys.argv:
        hot = int(sys.argv[sys.argv.index("--hot") + 1])

    rows = []
    base = []
    for line in sys.stdin:
        line = line.strip()
        m = RA.match(line)
        if m:
            name = m.group(1)
            v, r, i, a, s, c, n = (int(x) for x in m.groups()[1:])
            rows.append((name, v, r, i, a, s, c, n))
            continue
        m = BASE.match(line)
        if m:
            base.append((m.group(1), m.group(2), int(m.group(3))))

    if not rows and not base:
        print("no RA lines -- was FIRN_RA_STATS=1 set?")
        return 1

    tv = sum(r[1] for r in rows)
    tr = sum(r[2] for r in rows)
    ti = sum(r[3] for r in rows)
    ta = sum(r[4] for r in rows)
    ts = sum(r[5] for r in rows)
    tc = sum(r[6] for r in rows)
    tn = sum(r[7] for r in rows)
    bn = sum(b[2] for b in base)

    print("register allocation over %d functions" % (len(rows) + len(base)))
    print("  allocated (linear scan):   %5d functions, %7d instructions"
          % (len(rows), tn))
    print("  base path (no allocation): %5d functions, %7d instructions  (%.1f %% of the code)"
          % (len(base), bn, 100.0 * bn / max(1, tn + bn)))
    if tv:
        print("  values %d: %d in registers (%.1f %%), %d immediate, %d frame address, "
              "%d SPILLED (%.1f %%)"
              % (tv, tr, 100.0 * tr / tv, ti, ta, ts, 100.0 * ts / tv))
        print("  promoted alloca cells: %d" % tc)

    if base:
        why = {}
        for _, reason, n in base:
            why.setdefault(reason, [0, 0])
            why[reason][0] += 1
            why[reason][1] += n
        print("  why the base path:")
        for reason, (cnt, n) in sorted(why.items(), key=lambda kv: -kv[1][1]):
            print("    %-28s %4d functions, %7d instructions" % (reason, cnt, n))

    if hot:
        print("  the %d biggest allocated functions (by instruction count):" % hot)
        for name, v, r, i, a, s, c, n in sorted(rows, key=lambda x: -x[7])[:hot]:
            pct = 100.0 * s / v if v else 0.0
            print("    %-42s insts=%6d values=%5d regs=%4d spilled=%5d (%.1f %%)"
                  % (name[:42], n, v, r, s, pct))
    return 0


if __name__ == "__main__":
    sys.exit(main())
