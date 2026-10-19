import numpy as np

N = 1500
i, j = np.indices((N, N), dtype=np.int64)
A = 1.0 / ((i + j) * (i + j + 1) // 2 + i + 1)
u = np.ones(N)
for _ in range(10):
    v = A.T @ (A @ u)
    u = A.T @ (A @ v)
print(int(np.sqrt((u @ v) / (v @ v)) * 1e9))
