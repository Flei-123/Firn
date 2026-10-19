import os

STEPS = 200_000_000 // int(os.environ.get("PYN_SCALE", "1") or 1)
M = 0xFFFFFFFFFFFFFFFF
x, acc = 88172645463325252, 0
for i in range(STEPS):
    x ^= (x << 13) & M
    x ^= x >> 7
    x ^= (x << 17) & M
    k = (i & 63) | 1
    acc = ((acc + x.bit_count()) & M) ^ (((x << k) | (x >> (64 - k))) & M)
print(acc)
