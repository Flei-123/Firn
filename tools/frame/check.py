#!/usr/bin/env python3
"""Check that every x86-64 function of an --emit=asm listing keeps the save
slots of the callee-saved registers (rbx, r12-r15) 8-aligned to rbp.

The conservative stack scan of the collector reads aligned words only; a slot at
rbp-0x49 hides a Gc pointer that lives in the caller's callee-saved register
(r476). Usage: check.py <asm file> [--require-odd NAME...]
Exit 0 = ok, 1 = misaligned slot, 2 = nothing checked (listing format changed).
"""
import re
import sys

SAVE = re.compile(r"^\s+mov qword ptr \[rbp-(\d+)\], (rbx|r12|r13|r14|r15)\s*$")
FUNC = re.compile(r"^([A-Za-z_.$][\w.$@]*):\s*$")


def main():
    path = sys.argv[1]
    required = set(sys.argv[3:]) if len(sys.argv) > 2 and sys.argv[2] == "--require-odd" else set()
    funcs = 0
    saves = 0
    bad = []
    seen = {}
    cur = None
    in_prologue = False
    for line in open(path, encoding="utf-8", errors="replace"):
        line = line.rstrip("\n")
        m = FUNC.match(line)
        if m and not m.group(1).startswith(".L"):
            cur = m.group(1)
            funcs += 1
            in_prologue = True
            seen[cur] = [0, 0]
            continue
        if cur is None:
            continue
        if line.startswith(".L"):
            in_prologue = False
            continue
        if not in_prologue:
            continue
        m = SAVE.match(line)
        if m:
            off = int(m.group(1))
            saves += 1
            seen[cur][0] += 1
            if off % 8:
                seen[cur][1] += 1
                bad.append("%s: %s saved at rbp-%d (not 8-aligned)" % (cur, m.group(2), off))
    print("functions %d, callee-saved saves %d, misaligned %d" % (funcs, saves, len(bad)))
    for b in bad[:20]:
        print("  BAD", b)
    for name in sorted(required):
        hit = [k for k in seen if k.endswith("." + name) or k == name]
        if not hit or not any(seen[k][0] >= 3 for k in hit):
            print("  MISSING: %s has no callee-saved saves (test would prove nothing)" % name)
            return 2
    if funcs == 0 or saves == 0:
        print("  nothing checked -- asm listing format changed?")
        return 2
    return 1 if bad else 0


sys.exit(main())
