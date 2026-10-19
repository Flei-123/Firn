import os
from math import sqrt

S = int(os.environ.get("PYN_SCALE", "1") or 1)   # N = 1500 // S, time scales with N**2
N = 1500 // S


def eval_a(i, j):
    return 1.0 / ((i + j) * (i + j + 1) // 2 + i + 1)


def mul_av(src, dst):
    for i in range(N):
        s = 0.0
        for j in range(N):
            s = s + eval_a(i, j) * src[j]
        dst[i] = s


def mul_atv(src, dst):
    for i in range(N):
        s = 0.0
        for j in range(N):
            s = s + eval_a(j, i) * src[j]
        dst[i] = s


def mul_atav(src, dst, tmp):
    mul_av(src, tmp)
    mul_atv(tmp, dst)


u = [1.0] * N
v = [0.0] * N
t = [0.0] * N
for _ in range(10):
    mul_atav(u, v, t)
    mul_atav(v, u, t)
vbv = 0.0
vv = 0.0
for k in range(N):
    vbv = vbv + u[k] * v[k]
    vv = vv + v[k] * v[k]
print(int(sqrt(vbv / vv) * 1000000000.0))
