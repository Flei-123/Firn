import os
from math import sqrt

x, s = 1.0, 0.0
for i in range(100_000_000 // int(os.environ.get("PYN_SCALE", "1") or 1)):
    t = i * 0.0000001 + 1.0
    x = sqrt(x * x * 0.5 + t)
    s = s + x * 0.5 + 1.0 / (x + t)
print(int(s + 0.5))
