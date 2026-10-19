import os

n = int(os.environ.get("PYN_N", "10"))
perm1, count = list(range(n)), [0] * n
r, perm_count, checksum, max_flips = n, 0, 0, 0
while True:
    while r != 1:
        count[r - 1] = r
        r -= 1
    perm, flips, k = perm1[:], 0, perm1[0]
    while k:
        perm[:k + 1] = perm[k::-1]
        flips += 1
        k = perm[0]
    max_flips = max(max_flips, flips)
    checksum += flips if perm_count % 2 == 0 else -flips
    while True:
        if r == n:
            print(checksum * 1000 + max_flips)
            raise SystemExit
        perm1.insert(r, perm1.pop(0))
        count[r] -= 1
        if count[r] > 0:
            break
        r += 1
    perm_count += 1
