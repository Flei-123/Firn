import os

M, A, C = 0xFFFFFFFFFFFFFFFF, 6364136223846793005, 1442695040888963407
PIECES = 1_000_000 // int(os.environ.get("PYN_SCALE", "1") or 1)
total, s = 0, 7
for _ in range(3):
    parts = []
    for _ in range(PIECES):
        s = (s * A + C) & M
        parts.append(str(s >> 40))
    buf = (",".join(parts) + ",").encode()
    h = 14695981039346656037
    for b in buf:
        h = ((h ^ b) * 1099511628211) & M
    total = (total * 1000003 + (h ^ len(buf))) & M
print(total)
