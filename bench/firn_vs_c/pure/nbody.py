import os
from math import sqrt

S = int(os.environ.get("PYN_SCALE", "1") or 1)
STEPS = 4_000_000 // S
SOLAR_MASS = 4.0 * 3.141592653589793 * 3.141592653589793
DAYS = 365.24
PX = [0.0] * 5
PY = [0.0] * 5
PZ = [0.0] * 5
VX = [0.0] * 5
VY = [0.0] * 5
VZ = [0.0] * 5
MS = [0.0] * 5


def set_body(i, x, y, z, vx, vy, vz, m):
    PX[i] = x
    PY[i] = y
    PZ[i] = z
    VX[i] = vx * DAYS
    VY[i] = vy * DAYS
    VZ[i] = vz * DAYS
    MS[i] = m * SOLAR_MASS


def init():
    MS[0] = SOLAR_MASS
    set_body(1, 4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
             1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05,
             9.54791938424326609e-04)
    set_body(2, 8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
             -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05,
             2.85885980666130812e-04)
    set_body(3, 1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
             2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05,
             4.36624404335156298e-05)
    set_body(4, 1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
             2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05,
             5.15138902046611451e-05)
    px = 0.0
    py = 0.0
    pz = 0.0
    for i in range(5):
        px = px + VX[i] * MS[i]
        py = py + VY[i] * MS[i]
        pz = pz + VZ[i] * MS[i]
    VX[0] = 0.0 - px / SOLAR_MASS
    VY[0] = 0.0 - py / SOLAR_MASS
    VZ[0] = 0.0 - pz / SOLAR_MASS


def advance(dt):
    for i in range(5):
        for j in range(i + 1, 5):
            dx = PX[i] - PX[j]
            dy = PY[i] - PY[j]
            dz = PZ[i] - PZ[j]
            d2 = dx * dx + dy * dy + dz * dz
            mag = dt / (d2 * sqrt(d2))
            VX[i] = VX[i] - dx * MS[j] * mag
            VY[i] = VY[i] - dy * MS[j] * mag
            VZ[i] = VZ[i] - dz * MS[j] * mag
            VX[j] = VX[j] + dx * MS[i] * mag
            VY[j] = VY[j] + dy * MS[i] * mag
            VZ[j] = VZ[j] + dz * MS[i] * mag
    for k in range(5):
        PX[k] = PX[k] + dt * VX[k]
        PY[k] = PY[k] + dt * VY[k]
        PZ[k] = PZ[k] + dt * VZ[k]


def energy():
    e = 0.0
    for i in range(5):
        e = e + 0.5 * MS[i] * (VX[i] * VX[i] + VY[i] * VY[i] + VZ[i] * VZ[i])
        for j in range(i + 1, 5):
            dx = PX[i] - PX[j]
            dy = PY[i] - PY[j]
            dz = PZ[i] - PZ[j]
            e = e - MS[i] * MS[j] / sqrt(dx * dx + dy * dy + dz * dz)
    return e


init()
for _ in range(STEPS):
    advance(0.01)
print(int(energy() * 1000000000.0))
