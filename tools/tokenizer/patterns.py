#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Dynamically weighted INSTRUCTION PATTERNS of a Firn binary (round 51).

WHY: `tools/tokenizer/profile.py` answers "which FUNCTION costs?".
This file answers the question next to it -- "which FORM of code costs?".
Only both together show where the compiler works badly.

The lesson of round 43 (5) was: **static frequency is no estimate
of the gain** -- there a static counter was too optimistic by a factor of
eight. That is why this tool combines the disassembly with the
INSTRUCTION-EXACT callgrind output and weights every pattern with its
real execution count.

Producing the inputs:

    objdump -d --no-show-raw-insn BINARY > dis.txt
    valgrind --tool=callgrind --dump-instr=yes --cache-sim=no --branch-sim=no \
             --callgrind-out-file=cg.out BINARY < input

Usage:

    python3 tools/tokenizer/patterns.py dis.txt cg.out

TRAPS when reading the callgrind file (both have already struck here):
  * With `--dump-instr=yes` a cost line begins with the ADDRESS, which can
    also be relative (`+12`, `-4`, `*`). Whoever does not resolve that gets a
    plausible looking, wrong profile.
  * The line immediately after `calls=` is the INCLUSIVE cost of the
    call and does not belong to the self costs of the calling function.

This tool found the three largest items of round 51:
`setcc` chains (17.13 %), store+reload of the same cell (10.27 %) and
address arithmetic that could go into the memory operand (6.94 %).
"""
import re
import sys
from collections import defaultdict


def read_dis(path):
    """addr -> (Mnemonic, Operanden, Symbol)"""
    code = {}
    addrs = []
    sym = "?"
    for z in open(path, errors="replace"):
        m = re.match(r"^([0-9a-f]+) <(.+)>:", z)
        if m:
            sym = m.group(2)
            continue
        m = re.match(r"^\s+([0-9a-f]+):\t(\S+)\s*(.*)$", z.rstrip("\n"))
        if not m:
            continue
        a = int(m.group(1), 16)
        code[a] = (m.group(2), m.group(3).strip(), sym)
        addrs.append(a)
    addrs.sort()
    return code, addrs


def read_costs(path):
    """addr -> Ir (Selbstkosten)"""
    kosten = defaultdict(int)
    letzte = 0
    in_call = False
    for z in open(path, errors="replace"):
        if z.startswith(("calls=", "jump=", "jcnd=")):
            in_call = z.startswith("calls=")
            continue
        m = re.match(r"^(0x[0-9a-f]+|\+\d+|-\d+|\*)\s+(\S+)\s+(\d+)", z)
        if not m:
            if z.startswith(("fn=", "fl=", "cfn=", "cfl=", "cob=", "ob=")):
                in_call = False
            continue
        p = m.group(1)
        if p.startswith("0x"):
            a = int(p, 16)
        elif p == "*":
            a = letzte
        else:
            a = letzte + int(p)
        letzte = a
        if in_call:
            in_call = False   # the inclusive cost of the call, not the self cost
            continue
        kosten[a] += int(m.group(3))
    return kosten


def ist_bedingt(mn):
    return mn.startswith("j") and mn != "jmp"


def stamm(r):
    r = r.lstrip("%")
    fest = {
        "al": "rax", "ax": "rax", "eax": "rax", "bl": "rbx", "bx": "rbx", "ebx": "rbx",
        "cl": "rcx", "cx": "rcx", "ecx": "rcx", "dl": "rdx", "dx": "rdx", "edx": "rdx",
        "sil": "rsi", "si": "rsi", "esi": "rsi", "dil": "rdi", "di": "rdi", "edi": "rdi",
    }
    if r in fest:
        return fest[r]
    if re.match(r"^r\d+[dwb]$", r):
        return r[:-1]
    return r


def main():
    code, addrs = read_dis(sys.argv[1])
    kosten = read_costs(sys.argv[2])
    total = sum(kosten.values())
    if total == 0:
        print("keine Kosten gefunden — wurde --dump-instr=yes gesetzt?")
        return 1
    print(f"total (self costs from the instruction file): {total:,}")

    patterns = defaultdict(lambda: [0, 0])   # Name -> [Ir, Stellen]

    def zaehle(name, ir):
        m = patterns[name]
        m[0] += ir
        m[1] += 1

    for i, a in enumerate(addrs):
        mn, ops, sym = code[a]
        nxt = addrs[i + 1] if i + 1 < len(addrs) else None

        # (1) an unconditional jump directly behind a conditional one -> block layout
        if ist_bedingt(mn) and nxt is not None and code[nxt][0] == "jmp" and code[nxt][2] == sym:
            zaehle("jmp direkt hinter jcc (Blocklayout)", kosten.get(nxt, 0))

        # (2) a setcc chain instead of a direct jump
        if mn.startswith("set"):
            kette = [a]
            j = i + 1
            hit = False
            while j < len(addrs) and j <= i + 5:
                b = addrs[j]
                mb = code[b][0]
                kette.append(b)
                if mb == "test":
                    if j + 1 < len(addrs) and ist_bedingt(code[addrs[j + 1]][0]):
                        hit = True
                    break
                if mb not in ("movzbl", "movzwl", "mov"):
                    break
                j += 1
            if hit:
                zaehle("setcc-Kette statt direktem Sprung",
                       sum(kosten.get(x, 0) for x in kette))

        # (3) storing and immediately loading the same cell again
        if mn == "mov" and ops.startswith("%") and "," in ops:
            q, z = ops.rsplit(",", 1)
            if "(%rbp)" in z and "(" not in q and nxt is not None:
                m2, o2, _ = code[nxt]
                if m2 in ("mov", "movzbl", "movzwl", "movslq") and o2.startswith(z + ","):
                    zaehle("Store+Reload derselben Zelle",
                           kosten.get(a, 0) + kosten.get(nxt, 0))

        # (4) address arithmetic that could go into the memory operand
        if mn == "lea" and nxt is not None:
            m = re.match(r"^(-?0x[0-9a-f]+)?\((%r[a-z0-9]+)(,(%r[a-z0-9]+),(\d))?\),(%r[a-z0-9]+)$", ops)
            if m and code[nxt][0].startswith("mov") and f"({m.group(6)})" in code[nxt][1]:
                zaehle("lea + Zugriff (Adressierungsmodus ungenutzt)", kosten.get(a, 0))

        # (5) frame management
        if mn in ("push", "pop", "ret"):
            zaehle("Rahmenverwaltung (push/pop/ret)", kosten.get(a, 0))
        elif mn == "call":
            zaehle("Rahmenverwaltung (call)", kosten.get(a, 0))
        elif mn == "mov" and ops in ("%rsp,%rbp", "%rbp,%rsp"):
            zaehle("Rahmenverwaltung (rsp<->rbp)", kosten.get(a, 0))
        elif mn == "mov" and "," in ops:
            q, z = ops.rsplit(",", 1)
            gesichert = ("%r12", "%r13", "%r14", "%r15", "%rbx")
            if (q in gesichert and "(%rbp)" in z) or (z in gesichert and "(%rbp)" in q):
                zaehle("Rahmenverwaltung (callee-saved sichern/holen)", kosten.get(a, 0))

    print("\n== patterns (dynamically weighted) ==")
    for name, (ir, n) in sorted(patterns.items(), key=lambda x: -x[1][0]):
        print(f"  {name:<48} {ir:>14,} Ir  {100 * ir / total:6.2f}%   {n:>6} Stellen")

    proMn = defaultdict(int)
    proN = defaultdict(int)
    for a in addrs:
        proMn[code[a][0]] += kosten.get(a, 0)
        proN[code[a][0]] += 1
    print("\n== Top-Mnemonics ==")
    for mn, ir in sorted(proMn.items(), key=lambda x: -x[1])[:20]:
        print(f"  {mn:<12} {ir:>14,} Ir  {100 * ir / total:6.2f}%   {proN[mn]:>6} statisch")

    print("\n== data movements by kind ==")
    kind = defaultdict(int)
    artn = defaultdict(int)
    for a in addrs:
        mn, ops, _ = code[a]
        if not mn.startswith("mov") or "," not in ops:
            continue
        q, z = ops.rsplit(",", 1)
        f = lambda o: "memory" if "(" in o else ("constant" if o.startswith("$") else "register")
        kind[f"{f(q)} -> {f(z)}"] += kosten.get(a, 0)
        artn[f"{f(q)} -> {f(z)}"] += 1
    for name, ir in sorted(kind.items(), key=lambda x: -x[1]):
        print(f"  {name:<28} {ir:>14,} Ir  {100 * ir / total:6.2f}%   {artn[name]:>6} statisch")
    return 0


if __name__ == "__main__":
    sys.exit(main())
