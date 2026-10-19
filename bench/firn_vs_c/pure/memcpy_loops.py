import os

S = int(os.environ.get("PYN_SCALE", "1") or 1)   # rounds = 12 // S
M64 = 0xFFFFFFFFFFFFFFFF
n = 16_777_216
words = n // 8
a = bytearray(n)
b = bytearray(n)
c = bytearray(n)
aw = memoryview(a).cast("Q")
bw = memoryview(b).cast("Q")
cw = memoryview(c).cast("Q")
acc = 0
for r in range(12 // S):
    for j in range(words):
        aw[j] = ((j + r) * 0x9E3779B97F4A7C15) & M64
    for i in range(n):
        b[i] = a[i]
    for i in range(n // 2):
        a[i] = (i + r) & 255
    for j in range(words):
        cw[j] = bw[j]
    for j in range(0, words, 4099):
        acc = (acc * 31 + cw[j]) & M64
for j in range(words):
    acc = (acc * 31 + (aw[j] ^ cw[j])) & M64
print(acc)
