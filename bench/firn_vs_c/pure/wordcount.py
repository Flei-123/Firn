import os

S = int(os.environ.get("PYN_SCALE", "1") or 1)
M64 = 0xFFFFFFFFFFFFFFFF
A = 6364136223846793005
C = 1442695040888963407
KEYS = [0] * 16384
COUNTS = [0] * 16384


def add_word(h):
    slot = (h >> 20) & 16383
    while True:
        k = KEYS[slot]
        if k == h:
            COUNTS[slot] += 1
            return
        if k == 0:
            KEYS[slot] = h
            COUNTS[slot] = 1
            return
        slot = (slot + 1) & 16383


nwords = 3_000_000 // S
text = bytearray()
s = 99
for w in range(nwords):
    s = (s * A + C) & M64
    length = 1 + ((s >> 59) % 6)
    for c in range(length):
        s = (s * A + C) & M64
        text.append(97 + (s >> 62))
    text.append(32)
for _ in range(3):
    h = 14695981039346656037
    for i in range(len(text)):
        b = text[i]
        if b == 32:
            add_word(h)
            h = 14695981039346656037
        else:
            h = ((h ^ b) * 1099511628211) & M64
acc = 0
distinct = 0
for j in range(16384):
    if KEYS[j] != 0:
        distinct += 1
    acc = (acc * 31 + (KEYS[j] ^ ((COUNTS[j] * 1000003) & M64))) & M64
print((acc * 1000003 + distinct) & M64)
