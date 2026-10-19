import numpy as np

N = 700
i, j = np.indices((N, N), dtype=np.int64)
C = ((i * 31 + j * 17 + 3) % 101) @ ((i * 13 + j * 7 + 5) % 97)
h = 0
for c in C.ravel().tolist():
    h = (h * 1000003 + c) & 0xFFFFFFFFFFFFFFFF
print(h)
