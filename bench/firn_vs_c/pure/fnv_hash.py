import os

S = int(os.environ.get("PYN_SCALE", "1") or 1)
M64 = 0xFFFFFFFFFFFFFFFF
A = 6364136223846793005
C = 1442695040888963407
n = 67_108_864 // S
buf = bytearray(n)
s = 1
for i in range(n):
    s = (s * A + C) & M64
    buf[i] = s >> 56
h = 14695981039346656037
for _ in range(4):
    for i in range(n):
        h = ((h ^ buf[i]) * 1099511628211) & M64
print(h)
