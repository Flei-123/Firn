import os

S = int(os.environ.get("PYN_SCALE", "1") or 1)
STEPS = 200_000_000 // S
M64 = 0xFFFFFFFFFFFFFFFF


def popcount(v):
    x = v
    x = x - ((x >> 1) & 0x5555555555555555)
    x = (x & 0x3333333333333333) + ((x >> 2) & 0x3333333333333333)
    x = (x + (x >> 4)) & 0x0F0F0F0F0F0F0F0F
    return ((x * 0x0101010101010101) & M64) >> 56


def rotl(v, k):
    return ((v << k) | (v >> (64 - k))) & M64


x = 88172645463325252
acc = 0
for i in range(STEPS):
    x = x ^ ((x << 13) & M64)
    x = x ^ (x >> 7)
    x = x ^ ((x << 17) & M64)
    acc = (acc + popcount(x)) & M64
    acc = acc ^ rotl(x, (i & 63) | 1)
print(acc)
