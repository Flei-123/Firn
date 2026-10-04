#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/prof.py <binary> <callgrind.out> [top] -- callgrind's report with function names:
# the Firn compiler writes no function symbols that callgrind understands, so the
# addresses are looked up in `nm -n` of the binary.
import bisect, re, subprocess, sys

binary, out = sys.argv[1], sys.argv[2]
top = int(sys.argv[3]) if len(sys.argv) > 3 else 40
syms = []
for line in subprocess.run(["nm", "-n", binary], capture_output=True, text=True).stdout.splitlines():
    parts = line.split()
    if len(parts) == 3 and parts[1] in "tT":
        syms.append((int(parts[0], 16), parts[2]))
addrs = [a for a, _ in syms]
rep = subprocess.run(["callgrind_annotate", out], capture_output=True, text=True).stdout
total = {}
for line in rep.splitlines():
    m = re.match(r"\s*([\d,]+)\s+\(\s*[\d.]+%\)\s+\S+:0x([0-9a-f]+)", line)
    if not m:
        continue
    ir = int(m.group(1).replace(",", ""))
    a = int(m.group(2), 16)
    i = bisect.bisect_right(addrs, a) - 1
    name = syms[i][1] if i >= 0 else "?"
    total[name] = total.get(name, 0) + ir
s = sum(total.values())
for name, ir in sorted(total.items(), key=lambda kv: -kv[1])[:top]:
    print(f"{ir:>14,}  {100 * ir / s:5.1f}%  {name}")
