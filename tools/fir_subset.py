#!/usr/bin/env python3
# tools/fir_subset.py A B -- exit 0 when every function of B (one-file dump) appears byte-identical in A
# (firnc0 merged program, which also holds runtime functions pulled in by the implicit gc import).
# Prints the instruction count of the compared functions.
import sys


def blocks(path):
    out = {}
    name = None
    cur = []
    for line in open(path).read().split("\n"):
        if line.startswith("fn @"):
            name = line.split("(")[0]
            cur = [line]
        elif name is not None:
            cur.append(line)
            if line == "}":
                out[name] = "\n".join(cur)
                name = None
    return out


a = blocks(sys.argv[1])
b = blocks(sys.argv[2])
if not b:
    sys.exit(1)
count = 0
for k, v in b.items():
    if a.get(k) != v:
        sys.exit(1)
    count += sum(1 for l in v.split("\n") if l.startswith("  "))
print(count)
