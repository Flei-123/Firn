import sys


def load(path):
    cases = {}
    cur = None
    for line in open(path):
        if line.startswith("C "):
            cur = int(line.split()[1])
            cases[cur] = []
        else:
            cases[cur].append(tuple(int(t) for t in line.split()))
    return cases


def spread(x, y, z):
    h = ((y + z * 31) * 31 + x) & 0xFFFFFFFF
    return h ^ (h >> 16)


ref = load(sys.argv[1])
got = load(sys.argv[2])
bad = []
skipped = 0
for case, rows in ref.items():
    hashes = [spread(*p) for p in rows]
    if len(set(hashes)) != len(hashes):
        skipped += 1
        continue
    if rows != got.get(case):
        bad.append(case)
checked = len(ref) - skipped
if bad:
    print(f"  FAIL  jset order: {len(bad)} of {checked} cases differ: {bad[:20]}")
    sys.exit(1)
print(f"  ok    jset order: {checked} cases identical to java.util.HashSet ({skipped} skipped: equal hashes use identityHashCode)")
