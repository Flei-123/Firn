import numpy as np

SIZE, MAX_ITER = 800, 500
cr = np.broadcast_to(-2.0 + 3.0 * np.arange(SIZE) / SIZE, (SIZE, SIZE))
ci = np.broadcast_to((-1.5 + 3.0 * np.arange(SIZE) / SIZE)[:, None], (SIZE, SIZE))
zr = np.zeros((SIZE, SIZE))
zi = np.zeros((SIZE, SIZE))
its = np.zeros((SIZE, SIZE), dtype=np.int64)
for _ in range(MAX_ITER):
    zr2, zi2 = zr * zr, zi * zi
    alive = zr2 + zi2 <= 4.0
    if not alive.any():
        break
    its += alive
    zi = np.where(alive, 2.0 * zr * zi + ci, zi)
    zr = np.where(alive, zr2 - zi2 + cr, zr)
    # escaped pixels stay frozen: zr2+zi2 > 4 stays true for them
print(int(its.sum()))
