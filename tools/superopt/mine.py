import collections
import json
import sys

from live import flags_dead_after, reg_dead_after
from x86 import normalize

MAX_WINDOW = 6


def eligible(line):
    return normalize([line]) is not None


def mine_file(path, counts):
    lines = open(path, errors="replace").read().split("\n")
    n = len(lines)
    i = 0
    while i < n:
        if not eligible(lines[i]):
            i += 1
            continue
        j = i
        while j < n and eligible(lines[j]):
            j += 1
        for a in range(i, j):
            for b in range(a + 1, min(j, a + MAX_WINDOW) + 1):
                norm = normalize(lines[a:b])
                if norm is None:
                    continue
                pat, regs = norm
                dead = 0
                for v, r in enumerate(regs):
                    if reg_dead_after(lines, b, r):
                        dead |= 1 << v
                key = ("\n".join(pat), dead, flags_dead_after(lines, b))
                counts[key] += 1
        i = j


def main():
    out = sys.argv[1]
    counts = collections.Counter()
    for p in sys.argv[2:]:
        mine_file(p, counts)
    rows = [{"pattern": k[0].split("\n"), "dead": k[1], "flags_dead": k[2], "count": c}
            for k, c in counts.most_common()]
    json.dump(rows, open(out, "w"), indent=0)
    print("windows", sum(counts.values()), "distinct", len(counts))


if __name__ == "__main__":
    main()
