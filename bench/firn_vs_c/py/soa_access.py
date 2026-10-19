import numpy as np

n, M, A, C = 1_000_000, 0xFFFFFFFFFFFFFFFF, 6364136223846793005, 1442695040888963407
vals, s = [], 5
for _ in range(4 * n):
    s = (s * A + C) & M
    vals.append(s >> 20)
x, y, z, w = np.array(vals, dtype=np.uint64).reshape(n, 4).T.copy()   # SoA == AoS here: same data, same kernels
for _ in range(8):
    x += y * np.uint64(3) + (z >> np.uint64(1))
    w ^= x
for _ in range(24):
    w += x
pw = np.cumprod(np.full(n, 31, np.uint64))      # 31^1..31^n (wraps mod 2^64)
f = int(((x ^ w) * np.concatenate(([np.uint64(1)], pw[:-1]))[::-1]).sum())   # fold h = h*31 + v
print((f * 1000003 + f) & M)
