import os
import numpy as np

n, M = 16_777_216, 0xFFFFFFFFFFFFFFFF
W = n // 8
ROUNDS = 12 // int(os.environ.get("PYN_SCALE", "1") or 1)
a = np.zeros(W, np.uint64)
a8 = a.view(np.uint8)
acc = 0
for r in range(ROUNDS):
    a[:] = (np.arange(W, dtype=np.uint64) + np.uint64(r)) * np.uint64(0x9E3779B97F4A7C15)
    c = a.copy()                                          # byte copy a->b, word copy b->c
    a8[:n // 2] = (np.arange(n // 2) + r) & 255
    for j in range(0, W, 4099):
        acc = (acc * 31 + int(c[j])) & M
pw = np.cumprod(np.full(W, 31, np.uint64))               # 31^1..31^W
wts = np.concatenate(([np.uint64(1)], pw[:-1]))[::-1]    # fold h = h*31 + v as dot product
acc = (acc * pow(31, W, 1 << 64) + int(((a ^ c) * wts).sum())) & M
print(acc)
