# Reference for tests/4523_std_setops.fi (mirrors it step by step).
st = [31337]
def nxt():
    st[0] = (st[0] * 1103515245 + 12345) & 0x7fffffff
    return st[0]
def rnd(m): return (nxt() >> 8) % m
H = [7]
def feed(x): H[0] = (H[0] * 31 + (x & 0xffffffff)) & 0xffffffff
def feed_set(s):
    feed(len(s))
    for x in sorted(s): feed(x)
def mk(n, m): return set(rnd(m) for _ in range(n))
cnt = 0
for r in range(400):
    n1 = rnd(30); n2 = rnd(30); m = 1 + rnd(60)
    a = set(); b = set()
    for _ in range(n1): a.add(rnd(m))
    for _ in range(n2): b.add(rnd(m))
    for s in (a | b, a & b, a - b, a ^ b): feed_set(s)
    feed(1 if a <= b else 0); feed(1 if a >= b else 0); feed(1 if a.isdisjoint(b) else 0); feed(1 if a == b else 0)
    c = set(a); c |= b; feed_set(c)
    c = set(a); c &= b; feed_set(c)
    c = set(a); c -= b; feed_set(c)
    # subset-heavy cases
    d = set(x for x in sorted(a) if rnd(3) > 0)
    feed(1 if d <= a else 0); feed(1 if a >= d else 0); feed(1 if d == a else 0)
print("setops", H[0])
