import hashlib
import numpy as np

n, M = 16_777_216, 0xFFFFFFFFFFFFFFFF
A, C = 6364136223846793005, 1442695040888963407
s = np.empty(n, np.uint64)          # LCG stream via doubling (affine jump-ahead)
s[0] = (3 * A + C) & M
m, Am, Cm = 1, A, C
while m < n:
    k = min(m, n - m)
    s[m:m + k] = s[:k] * np.uint64(Am) + np.uint64(Cm)
    Cm, Am, m = (Am * Cm + Cm) & M, Am * Am & M, m * 2
buf = (s >> np.uint64(56)).astype(np.uint8).tobytes()
print(int.from_bytes(hashlib.sha256(buf).digest()[:8], "big"))
