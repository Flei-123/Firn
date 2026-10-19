M64 = 0xFFFFFFFFFFFFFFFF
A = 6364136223846793005
C = 1442695040888963407
MASK = 4194303


def slot_of(k):
    return ((k * 0x9E3779B97F4A7C15) & M64) >> 42


def put(t, k, v):
    s = slot_of(k)
    while True:
        cur = t[s * 2]
        if cur == 0 or cur == k:
            t[s * 2] = k
            t[s * 2 + 1] = v
            return
        s = (s + 1) & MASK


def get(t, k):
    s = slot_of(k)
    while True:
        cur = t[s * 2]
        if cur == k:
            return t[s * 2 + 1]
        if cur == 0:
            return 0
        s = (s + 1) & MASK


n = 2_000_000
t = [0] * (4194304 * 2)
s = 12345
for i in range(n):
    s = (s * A + C) & M64
    put(t, s | 1, i + 1)
total = 0
s = 12345
for i in range(n):
    s = (s * A + C) & M64
    total += get(t, s | 1)
s2 = 777
misses = 0
for i in range(n):
    s2 = (s2 * A + C) & M64
    v = get(t, s2 | 1)
    if v == 0:
        misses += 1
    else:
        total += v * 3
print(((total & M64) * 1000003 + misses) & M64)
