import os
import numpy as np

n, M = 67_108_864 // int(os.environ.get("PYN_SCALE", "1") or 1), 0xFFFFFFFFFFFFFFFF
A, C = 6364136223846793005, 1442695040888963407
s = np.empty(n, np.uint64)          # LCG stream via doubling (affine jump-ahead)
s[0] = (A + C) & M
m, Am, Cm = 1, A, C
while m < n:
    k = min(m, n - m)
    s[m:m + k] = s[:k] * np.uint64(Am) + np.uint64(Cm)
    Cm, Am, m = (Am * Cm + Cm) & M, Am * Am & M, m * 2
buf = (s >> np.uint64(56)).astype(np.uint8).tobytes()
h = 14695981039346656037
for _ in range(4):
    for b in buf:
        h = ((h ^ b) * 1099511628211) & M
print(h)
