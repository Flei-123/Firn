from collections import Counter

M, A, C = 0xFFFFFFFFFFFFFFFF, 6364136223846793005, 1442695040888963407
words, s = [], 99
for _ in range(3_000_000):
    s = (s * A + C) & M
    word = []
    for _ in range(1 + (s >> 59) % 6):
        s = (s * A + C) & M
        word.append(chr(97 + (s >> 62)))
    words.append("".join(word))
table = {}                      # FNV-1a hash -> count, in first-seen order
for w, cnt in Counter(words).items():
    h = 14695981039346656037
    for b in w.encode():
        h = ((h ^ b) * 1099511628211) & M
    table[h] = table.get(h, 0) + 3 * cnt        # 3 identical passes
keys, counts = [0] * 16384, [0] * 16384
for h, cnt in table.items():                    # open addressing, linear probing
    slot = (h >> 20) & 16383
    while keys[slot]:
        slot = (slot + 1) & 16383
    keys[slot], counts[slot] = h, cnt
acc = 0
for k, c in zip(keys, counts):
    acc = (acc * 31 + (k ^ (c * 1000003 & M))) & M
print((acc * 1000003 + len(table)) & M)
