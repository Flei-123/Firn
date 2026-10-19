import os

S = int(os.environ.get("PYN_SCALE", "1") or 1)
M64 = 0xFFFFFFFFFFFFFFFF
A = 6364136223846793005
C = 1442695040888963407
PIECES = 1_000_000 // S
buf = bytearray()


def push_dec(v):
    tmp = []
    x = v
    if x == 0:
        tmp.append(48)
    while x > 0:
        tmp.append(x % 10 + 48)
        x //= 10
    while tmp:
        buf.append(tmp.pop())


total = 0
s = 7
for rnd in range(3):
    buf = bytearray()
    for i in range(PIECES):
        s = (s * A + C) & M64
        push_dec(s >> 40)
        buf.append(44)
    h = 14695981039346656037
    for k in range(len(buf)):
        h = ((h ^ buf[k]) * 1099511628211) & M64
    total = (total * 1000003 + (h ^ len(buf))) & M64
print(total)
