#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Callgrind profile of a Firn binary with RESOLVED function names.

WHY: Firn binaries are static and have no dynamic section; callgrind
does not find the symbols and names every function only after its start
address (`fn=(748) 0x000000000041d33b`). But the names are very much in
`.symtab` (`nm` reads them).

FORMAT (important, this has already been parsed wrongly once): with
`positions: line` the FIRST number of a cost line is the LINE NUMBER,
NOT the address. The address of a function stands exclusively in the
`fn=` head. The costs are therefore attributed to the last named `fn`.
`fn=(id)` without a name refers to an id introduced earlier.

What is printed are the SELF COSTS (cost lines of the function itself; the
line after `calls=` is the INCLUSIVE cost of the call and is skipped)
and in addition the INCLUSIVE COSTS per function (the sum of the `calls=`
lines pointing at it; with recursion that overcounts and is only a hint).

Usage:  python3 tools/tokenizer/profile.py <binary> <callgrind-out> [count]
"""
import subprocess
import sys
import bisect
import re


def symbole(binary):
    aus = subprocess.run(["nm", "-n", binary], capture_output=True, text=True)
    tab = []
    for z in aus.stdout.splitlines():
        t = z.split()
        if len(t) != 3:
            continue
        adr, typ, name = t
        if typ.upper() not in ("T", "W"):
            continue
        try:
            tab.append((int(adr, 16), name))
        except ValueError:
            pass
    tab.sort()
    return tab


def finde(tab, starts, adr):
    k = bisect.bisect_right(starts, adr) - 1
    if k < 0:
        return "???"
    return tab[k][1]


KOPF = re.compile(r"^(c?fn)=\((\d+)\)(?:\s+(.*))?$")


def read_cg(path):
    """Self costs and call costs per function id."""
    names = {}          # id -> raw name (usually 0x...)
    self_cost = {}         # id -> Ir
    incl = {}       # id -> Ir (the sum of the calls THERE)
    akt = None
    target = None
    warte_aufrufkosten = False
    with open(path, "r", errors="replace") as f:
        for z in f:
            z = z.rstrip("\n")
            if not z:
                continue
            m = KOPF.match(z)
            if m:
                kind, ident, name = m.group(1), m.group(2), m.group(3)
                if name:
                    names[ident] = name.strip()
                if kind == "fn":
                    akt = ident
                    warte_aufrufkosten = False
                else:
                    target = ident
                continue
            if z.startswith("calls="):
                warte_aufrufkosten = True
                continue
            if z[0] in "0123456789+-*":
                t = z.split()
                if len(t) < 2:
                    continue
                try:
                    ir = int(t[1])
                except ValueError:
                    continue
                if warte_aufrufkosten:
                    warte_aufrufkosten = False
                    if target is not None:
                        incl[target] = incl.get(target, 0) + ir
                    continue
                if akt is not None:
                    self_cost[akt] = self_cost.get(akt, 0) + ir
                continue
            warte_aufrufkosten = False
    return names, self_cost, incl


def resolve(names, tab, starts):
    """id -> lesbarer Name."""
    aus = {}
    for ident, raw in names.items():
        if raw.startswith("0x"):
            try:
                aus[ident] = finde(tab, starts, int(raw, 16))
                continue
            except ValueError:
                pass
        aus[ident] = raw
    return aus


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 1
    binary, cg = sys.argv[1], sys.argv[2]
    count = int(sys.argv[3]) if len(sys.argv) > 3 else 30
    tab = symbole(binary)
    starts = [a for a, _ in tab]
    names, self_cost, incl = read_cg(cg)
    readable = resolve(names, tab, starts)

    s = {}
    for ident, v in self_cost.items():
        n = readable.get(ident, "???")
        s[n] = s.get(n, 0) + v
    i = {}
    for ident, v in incl.items():
        n = readable.get(ident, "???")
        i[n] = i.get(n, 0) + v

    ges = sum(s.values())
    print(f"{'SELBST':>16} {'ANTEIL':>8} {'INKLUSIV':>16}  FUNKTION")
    print("-" * 78)
    for n, v in sorted(s.items(), key=lambda kv: -kv[1])[:count]:
        print(f"{v:16,d} {v/ges*100:7.2f}% {i.get(n, 0):16,d}  {n}")
    print("-" * 78)
    print(f"{ges:16,d} {100.0:7.2f}%                    SUMME (Selbstkosten)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
