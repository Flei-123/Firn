import numpy as np

N = 20_000_000
sieve = np.ones(N + 1, dtype=bool)
sieve[:2] = False
for i in range(2, int(N ** 0.5) + 1):
    if sieve[i]:
        sieve[i * i::i] = False
print(int(np.flatnonzero(sieve).sum()))
