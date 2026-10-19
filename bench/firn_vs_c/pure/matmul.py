import os

S = int(os.environ.get("PYN_SCALE", "1") or 1)   # N = 700 // S, time scales with N**3
N = 700 // S
M64 = 0xFFFFFFFFFFFFFFFF
MA = [0] * (N * N)
MB = [0] * (N * N)
MC = [0] * (N * N)
for i in range(N):
    for j in range(N):
        MA[i * N + j] = (i * 31 + j * 17 + 3) % 101
        MB[i * N + j] = (i * 13 + j * 7 + 5) % 97
for r in range(N):
    for c in range(N):
        s = 0
        for k in range(N):
            s = s + MA[r * N + k] * MB[k * N + c]
        MC[r * N + c] = s
h = 0
for d in range(N * N):
    h = (h * 1000003 + MC[d]) & M64
print(h)
