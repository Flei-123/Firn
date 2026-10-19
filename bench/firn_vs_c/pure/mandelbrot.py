import os

S = int(os.environ.get("PYN_SCALE", "1") or 1)
SIZE = 800 // S
MAX_ITER = 500
total = 0
for py in range(SIZE):
    ci = -1.5 + 3.0 * py / SIZE
    for px in range(SIZE):
        cr = -2.0 + 3.0 * px / SIZE
        zr = 0.0
        zi = 0.0
        it = 0
        while it < MAX_ITER:
            zr2 = zr * zr
            zi2 = zi * zi
            if zr2 + zi2 > 4.0:
                break
            zi = 2.0 * zr * zi + ci
            zr = zr2 - zi2 + cr
            it += 1
        total += it
print(total)
