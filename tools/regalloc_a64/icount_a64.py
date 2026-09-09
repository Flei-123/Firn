#!/usr/bin/env python3
"""ROUND REGALLOC-A64 -- EXACTLY HOW MANY INSTRUCTIONS aarch64 EXECUTES.

This qemu (7.2, Debian) has no TCG instruction plugin, and valgrind does not
run aarch64 here. But qemu can be made to say the two halves of the answer:

  -d in_asm   prints every translation block ONCE, with its instructions
  -d exec     prints one line per block ENTRY ("Trace ... [...pc]")
  -d nochain  stops blocks being chained, so that trace is COMPLETE

So: block size from `in_asm`, execution count from `exec`, and the sum of
size x count is the number of instructions the program really executed.

Usage:  icount_a64.py <a64-binary> [--fn <symbol-prefix>] [--elf <file>]
Prints the total, and with --fn also the share inside that function.
"""
import re
import subprocess
import sys
import tempfile
import os

def run(binary, args):
    log = tempfile.NamedTemporaryFile(suffix=".qlog", delete=False)
    log.close()
    cmd = ["qemu-aarch64", "-d", "in_asm,exec,nochain", "-D", log.name, binary] + args
    subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return log.name

def parse(path):
    """-> (size per block start pc, execution count per block start pc)"""
    size = {}
    count = {}
    cur = None
    n = 0
    # in_asm blocks look like:
    #   ----------------
    #   IN:
    #   0x0000000000400114:  d2800000  mov x0, #0
    # exec lines look like:
    #   Trace 0: 0x7f... [00000000/0000000000400114/...]
    ins_re = re.compile(r'^0x([0-9a-f]+):')
    trace_re = re.compile(r'^Trace .*?/([0-9a-f]+)/')
    with open(path, errors="ignore") as f:
        for ln in f:
            m = trace_re.match(ln)
            if m:
                pc = int(m.group(1), 16)
                count[pc] = count.get(pc, 0) + 1
                continue
            if ln.startswith("IN:"):
                if cur is not None:
                    size[cur] = n
                cur = None
                n = 0
                continue
            m = ins_re.match(ln)
            if m:
                pc = int(m.group(1), 16)
                if cur is None:
                    cur = pc
                    n = 0
                n += 1
    if cur is not None:
        size[cur] = n
    return size, count

def symbols(binary):
    """-> list of (start, end, name) from the ELF symbol table."""
    out = subprocess.run(["aarch64-linux-gnu-nm", "-n", "--defined-only", binary],
                         capture_output=True, text=True).stdout
    syms = []
    for ln in out.splitlines():
        p = ln.split()
        if len(p) >= 3 and p[1].lower() in ("t", "w"):
            syms.append((int(p[0], 16), p[2]))
    syms.sort()
    out = []
    for i, (a, nm) in enumerate(syms):
        b = syms[i + 1][0] if i + 1 < len(syms) else a + (1 << 20)
        out.append((a, b, nm))
    return out

def main():
    binary = sys.argv[1]
    want = None
    args = []
    rest = sys.argv[2:]
    i = 0
    while i < len(rest):
        if rest[i] == "--fn":
            want = rest[i + 1]
            i += 2
        else:
            args.append(rest[i])
            i += 1
    log = run(binary, args)
    size, count = parse(log)
    os.unlink(log)
    total = 0
    for pc, c in count.items():
        total += size.get(pc, 0) * c
    print(f"total {total}")
    if want:
        syms = symbols(binary)
        per = {}
        for pc, c in count.items():
            n = size.get(pc, 0) * c
            for a, b, nm in syms:
                if a <= pc < b:
                    per[nm] = per.get(nm, 0) + n
                    break
        for nm, n in sorted(per.items(), key=lambda kv: -kv[1]):
            if want in nm:
                print(f"{nm} {n}")

main()
