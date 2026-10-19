import os

S = int(os.environ.get("PYN_SCALE", "1") or 1)
M64 = 0xFFFFFFFFFFFFFFFF
A = 6364136223846793005
C = 1442695040888963407
TABLE = [0] * 256
for i in range(256):
    c = i
    for _ in range(8):
        c = 0xEDB88320 ^ (c >> 1) if c & 1 else c >> 1
    TABLE[i] = c
n = 67_108_864 // S
buf = bytearray(n)
s = 1
for i in range(n):
    s = (s * A + C) & M64
    buf[i] = s >> 56
crc = 0xFFFFFFFF
for _ in range(3):
    for i in range(n):
        crc = TABLE[(crc ^ buf[i]) & 255] ^ (crc >> 8)
print(crc ^ 0xFFFFFFFF)
