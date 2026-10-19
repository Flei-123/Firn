M64 = 0xFFFFFFFFFFFFFFFF
A = 6364136223846793005
C = 1442695040888963407
n = 1_000_000
aos = [None] * n
sx = [0] * n
sy = [0] * n
sz = [0] * n
sw = [0] * n
s = 5
for i in range(n):
    s = (s * A + C) & M64
    x = s >> 20
    s = (s * A + C) & M64
    y = s >> 20
    s = (s * A + C) & M64
    z = s >> 20
    s = (s * A + C) & M64
    w = s >> 20
    aos[i] = [x, y, z, w]
    sx[i] = x
    sy[i] = y
    sz[i] = z
    sw[i] = w
for _ in range(8):
    for i in range(n):
        p = aos[i]
        nx = (p[0] + p[1] * 3 + (p[2] >> 1)) & M64
        p[0] = nx
        p[3] ^= nx
    for i in range(n):
        nx = (sx[i] + sy[i] * 3 + (sz[i] >> 1)) & M64
        sx[i] = nx
        sw[i] ^= nx
for _ in range(24):
    for i in range(n):
        p = aos[i]
        p[3] = (p[3] + p[0]) & M64
    for i in range(n):
        sw[i] = (sw[i] + sx[i]) & M64
fa = 0
fs = 0
for i in range(n):
    p = aos[i]
    fa = (fa * 31 + (p[0] ^ p[3])) & M64
    fs = (fs * 31 + (sx[i] ^ sw[i])) & M64
print((fa * 1000003 + fs) & M64)
