#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""TOCTOU gate: every tN function must read the shared symbol exactly once.

usage: check.py x86|a64|wasm <listing> [--mutate]
x86/a64 read the --emit=asm text, wasm reads wasm2wat output.
A function `tN` (not `tN_use`) must contain exactly ONE read of SH/SP
(a scalar load or one bulk copy). Two reads = a compiler-invented load.
"""
import re, sys

CAN = {}
for n in range(8, 16):
    for suf in ("d", "w", "b"):
        CAN[f"r{n}{suf}"] = f"r{n}"
for a, b in (("a", "ax"), ("b", "bx"), ("c", "cx"), ("d", "dx")):
    CAN.update({f"r{a}x": f"r{a}x", f"e{b}": f"r{a}x", b: f"r{a}x", f"{a}l": f"r{a}x"})
for r in ("si", "di", "bp", "sp"):
    CAN.update({f"e{r}": f"r{r}", r: f"r{r}", f"{r}l": f"r{r}"})
def can(r): return CAN.get(r, r)

def x86(lines):
    fn = None; res = {}; alias = set(); leas = {}
    for l in lines:
        s = l.strip()
        m = re.match(r"^(?:_F0\.)?(\w+):$", s)
        if m and not s.startswith(".L"):
            fn = m.group(1); res[fn] = 0; leas[fn] = 0; alias = set(); continue
        if fn is None or not s or s.startswith(("#", ".loc")): continue
        if s.endswith(":"): continue
        if re.match(r"lea (\w+), \[rip \+ \.Lstatic_S[HP]\]$", s):
            leas[fn] += 1
            alias.add(can(re.match(r"lea (\w+),", s).group(1))); continue
        m = re.match(r"mov (\w+), (\w+)$", s)
        if m and can(m.group(2)) in alias:
            alias.add(can(m.group(1))); continue
        if "ptr [rip + .Lstatic_S" in s: res[fn] += 1
        used = re.findall(r"\[(\w+)[\]+\-*]", s)
        if any(can(u) in alias for u in used) and not s.startswith("lea"): res[fn] += 1
        if s.startswith("rep movs"):
            if "rsi" in alias: res[fn] += 1
            alias -= {"rdi", "rsi", "rcx"}
        if s.startswith("call"):
            alias -= {"rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"}
        m = re.match(r"\w+ (\w+),", s)
        if m and not s.startswith(("cmp", "test", "push")):
            d = can(m.group(1))
            if d in alias and not re.match(r"mov \w+, \w+$", s): alias.discard(d)
            elif d in alias and re.match(r"mov \w+, (\w+)$", s) and can(re.match(r"mov \w+, (\w+)$", s).group(1)) not in alias: alias.discard(d)
    return {k: max(v, leas[k]) for k, v in res.items()}

def a64(lines):
    fn = None; res = {}
    for l in lines:
        s = l.strip()
        m = re.match(r"^(?:_F0\.)?(\w+):$", s)
        if m and not s.startswith(".L"): fn = m.group(1); res[fn] = 0; continue
        if fn and re.match(r"adrp \w+, \.Lstatic_S[HP]$", s): res[fn] += 1
    return res

def wasm(lines):
    fn = None; res = {}
    for i, l in enumerate(lines):
        m = re.match(r"\s+\(func (?:\$(?:_F0\.)?(\w+))?", l)
        if m: fn = m.group(1) or "?"; res[fn] = 0; continue
        if fn and re.match(r"\s+i64\.const (\d+)$", l) and i + 1 < len(lines) and "i32.wrap_i64" in lines[i + 1]:
            if 64 <= int(re.search(r"(\d+)", l).group(1)) < 65536: res[fn] += 1
    return res

kind, path = sys.argv[1], sys.argv[2]
mutate = "--mutate" in sys.argv
lines = open(path).read().split("\n")
res = {"x86": x86, "a64": a64, "wasm": wasm}[kind](lines)
bad = 0; checked = 0
for fn, n in sorted(res.items()):
    if not re.match(r"t\d$", fn): continue
    want = 1
    checked += 1
    if mutate: want += 1  # self-test: the gate must now FAIL
    if n != want:
        print(f"  FAIL {fn}: {n} read(s) of the shared symbol, want {want}"); bad += 1
if checked < 7: print("  FAIL: only", checked, "functions found"); bad += 1
print(f"{checked} functions checked, {bad} bad")
sys.exit(1 if bad else 0)
