def build(d):
    return (build(d - 1), build(d - 1)) if d else (None, None)


def check(t):
    return 1 if t[0] is None else 1 + check(t[0]) + check(t[1])


M, lo, hi = 0xFFFFFFFFFFFFFFFF, 4, 16
total = check(build(hi + 1))
long_lived = build(hi)
for d in range(lo, hi + 1, 4):
    chk = sum(check(build(d)) for _ in range(1 << (hi - d + lo)))
    total = (total * 1000003 + chk) & M
print((total * 1000003 + check(long_lived)) & M)
