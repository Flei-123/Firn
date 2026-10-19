# Reference for tests/4524_std_bisect.fi (mirrors it step by step).
import bisect
st = [555]
def nxt():
    st[0] = (st[0] * 1103515245 + 12345) & 0x7fffffff
    return st[0]
def rnd(m): return (nxt() >> 8) % m
H = [7]
def feed(x): H[0] = (H[0] * 31 + (x & 0xffffffff)) & 0xffffffff
for r in range(300):
    n = rnd(40); m = 1 + rnd(30)
    v = sorted(rnd(m) for _ in range(n))
    for _ in range(20):
        x = rnd(m + 2)
        feed(bisect.bisect_left(v, x)); feed(bisect.bisect_right(v, x))
        lo = rnd(n + 1); hi = lo + rnd(n - lo + 1)
        feed(bisect.bisect_left(v, x, lo, hi)); feed(bisect.bisect_right(v, x, lo, hi))
    w = list(v)
    for _ in range(15):
        x = rnd(m + 2)
        if rnd(2): bisect.insort_left(w, x)
        else: bisect.insort_right(w, x)
    for x in w: feed(x)
    # pairs (key, tag): insort_right keeps insertion order of equal keys
    p = []
    for t in range(15):
        k = rnd(m)
        i = bisect.bisect_right([a for a, b in p], k)
        p.insert(i, (k, t))
    for a, b in p: feed(a); feed(b)
    q = []
    for t in range(15):
        k = rnd(m)
        i = bisect.bisect_left([a for a, b in q], k)
        q.insert(i, (k, t))
    for a, b in q: feed(a); feed(b)
print("bisect", H[0])
