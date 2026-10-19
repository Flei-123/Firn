# Reference for tests/4520_std_itertools.fi: prints "NAME count ck" lines.
# The Firn test mirrors this script step by step (same LCG, same order).
import itertools as it
st = [4242]
def nxt():
    st[0] = (st[0] * 1103515245 + 12345) & 0x7fffffff
    return st[0]
def rnd(m):
    return (nxt() >> 8) % m
H = [7]
def feed(x):
    H[0] = (H[0] * 31 + x) & 0xffffffff
def reset():
    H[0] = 7
res = {}
def done(name, count):
    res[name] = (count, H[0])
    print(name, count, H[0])
    reset()
def feed_all(seq):
    for t in seq:
        for x in (t if isinstance(t, tuple) else (t,)):
            feed(x)

# A: combinatorics over pools of size 0..6
for name, fn, rmax, nmax in [
    ("permutations", lambda p, r: it.permutations(p, r), 7, 6),
    ("combinations", lambda p, r: it.combinations(p, r), 7, 6),
    ("cwr", lambda p, r: it.combinations_with_replacement(p, r), 6, 5),
    ("product", lambda p, r: it.product(p, repeat=r), 5, 5),
]:
    total = 0
    for n in range(nmax + 1):
        pool = [rnd(1000) for _ in range(n)]
        for r in range(rmax + 1):
            tl = list(fn(pool, r))
            total += len(tl)
            feed(len(tl))
            feed_all(tl)
    done(name, total)

# B: islice
total = 0
for _ in range(300):
    n = rnd(31); v = [rnd(1000) for _ in range(n)]
    start = rnd(36); s = rnd(45); stop = None if s >= 40 else s; step = 1 + rnd(5)
    out = list(it.islice(v, start, stop, step))
    total += len(out); feed(len(out)); feed_all(out)
done("islice", total)

# C: accumulate / pairwise / batched / zip / zip_longest
total = 0
for _ in range(200):
    n = rnd(25); v = [rnd(1000) for _ in range(n)]
    a = list(it.accumulate(v)); b = list(it.accumulate(v, max))
    c = list(it.accumulate(v, lambda x, y: x + y, initial=5))
    total += len(a) + len(b) + len(c)
    feed_all(a); feed_all(b); feed_all(c)
done("accumulate", total)
total = 0
for _ in range(200):
    n = rnd(25); v = [rnd(1000) for _ in range(n)]
    p = list(it.pairwise(v))
    total += len(p); feed(len(p)); feed_all(p)
    k = 1 + rnd(6)
    bs = [v[i:i + k] for i in range(0, len(v), k)]  # itertools.batched (3.12+)
    feed(len(bs))
    for bt in bs:
        feed(len(bt)); feed_all(bt)
    w = [rnd(1000) for _ in range(rnd(25))]
    z = list(zip(v, w)); zl = list(it.zip_longest(v, w, fillvalue=7))
    total += len(z) + len(zl)
    feed(len(z)); feed_all(z); feed(len(zl)); feed_all(zl)
done("pairwise_batched_zip", total)

# D: groupby / runs
total = 0
for _ in range(300):
    n = rnd(30); v = [rnd(12) for _ in range(n)]
    starts = []; pos = 0
    for k, g in it.groupby(v, key=lambda x: x // 3):
        starts.append(pos); pos += len(list(g))
    total += len(starts); feed(len(starts)); feed_all(starts)
    starts2 = []; pos = 0
    for k, g in it.groupby(v):
        starts2.append(pos); pos += len(list(g))
    feed(len(starts2)); feed_all(starts2)
done("groupby", total)

# E: takewhile / dropwhile / filterfalse / compress / cycle / repeat / chain
total = 0
for _ in range(300):
    n = rnd(30); v = [rnd(1000) for _ in range(n)]
    sel = [rnd(2) for _ in range(rnd(35))]
    t = list(it.takewhile(lambda x: x < 700, v)); d = list(it.dropwhile(lambda x: x < 300, v))
    f = list(it.filterfalse(lambda x: x % 2 == 1, v)); cp = list(it.compress(v, sel))
    cy = list(it.islice(it.cycle(v), rnd(40)))
    rp = list(it.repeat(rnd(1000), rnd(9)))
    w = [rnd(1000) for _ in range(rnd(10))]
    ch = list(it.chain(v, w))
    for L in (t, d, f, cp, cy, rp, ch):
        total += len(L); feed(len(L)); feed_all(L)
done("filters_chain", total)

# F: next/prev permutation (distinct and with duplicates)
total = 0
for base in ([0, 1, 2, 3, 4, 5], [1, 1, 2, 2, 3], [5, 5, 5], [3], []):
    seq = sorted(set(it.permutations(base)))
    total += len(seq); feed(len(seq))
    for t in seq:
        feed_all([t])
done("next_permutation", total)
