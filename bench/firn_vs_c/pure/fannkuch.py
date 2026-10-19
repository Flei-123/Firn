import os

N = int(os.environ.get("PYN_N", "10"))
PERM = [0] * N
PERM1 = list(range(N))
COUNT = [0] * N
r = N
perm_count = 0
checksum = 0
max_flips = 0
while True:
    while r != 1:
        COUNT[r - 1] = r
        r -= 1
    for c in range(N):
        PERM[c] = PERM1[c]
    flips = 0
    k = PERM[0]
    while k != 0:
        lo = 0
        hi = k
        while lo < hi:
            PERM[lo], PERM[hi] = PERM[hi], PERM[lo]
            lo += 1
            hi -= 1
        flips += 1
        k = PERM[0]
    if flips > max_flips:
        max_flips = flips
    if perm_count % 2 == 0:
        checksum += flips
    else:
        checksum -= flips
    more = True
    while more:
        if r == N:
            print(checksum * 1000 + max_flips)
            raise SystemExit
        p0 = PERM1[0]
        for m in range(r):
            PERM1[m] = PERM1[m + 1]
        PERM1[r] = p0
        COUNT[r] -= 1
        if COUNT[r] > 0:
            more = False
        else:
            r += 1
    perm_count += 1
