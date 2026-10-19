M, A, C = 0xFFFFFFFFFFFFFFFF, 6364136223846793005, 1442695040888963407
n, d, s = 2_000_000, {}, 12345
for i in range(1, n + 1):
    s = (s * A + C) & M
    d[s | 1] = i
total, s = 0, 12345
for _ in range(n):
    s = (s * A + C) & M
    total += d.get(s | 1, 0)
s, misses = 777, 0
for _ in range(n):
    s = (s * A + C) & M
    v = d.get(s | 1, 0)
    if v:
        total += v * 3
    else:
        misses += 1
print((total * 1000003 + misses) & M)
