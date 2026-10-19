import os

S = int(os.environ.get("PYN_SCALE", "1") or 1)
LIMIT = 20_000_000 // S

sieve = bytearray(LIMIT + 1)
i = 2
while i * i <= LIMIT:
    if sieve[i] == 0:
        for j in range(i * i, LIMIT + 1, i):
            sieve[j] = 1
    i += 1
total = 0
for k in range(2, LIMIT + 1):
    if sieve[k] == 0:
        total += k
print(total)
