from operator import mul

N = 2_000_000
a, x = [], 12345
for _ in range(N):
    x = (x * 1664525 + 1013904223) & 0xFFFFFFFF
    a.append(x)
a.sort()
print(sum(map(mul, a, range(1, N + 1))) & 0xFFFFFFFFFFFFFFFF)
