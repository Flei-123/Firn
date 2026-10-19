// Benchmark: n-body simulation, 5 bodies, 4,000,000 steps, dt = 0.01 (f64).
// Output = final energy * 1e9 as i64.
// Counterpart: bench/firn_vs_c/firn/nbody.fi
#include <stdio.h>
#include <stdint.h>
#include <math.h>

#define STEPS 4000000
#define SOLAR_MASS (4.0 * 3.141592653589793 * 3.141592653589793)
#define DAYS 365.24

static double PX[5], PY[5], PZ[5], VX[5], VY[5], VZ[5], MS[5];

static void set_body(int i, double x, double y, double z, double vx, double vy, double vz, double m) {
    PX[i] = x;
    PY[i] = y;
    PZ[i] = z;
    VX[i] = vx * DAYS;
    VY[i] = vy * DAYS;
    VZ[i] = vz * DAYS;
    MS[i] = m * SOLAR_MASS;
}

static void init(void) {
    PX[0] = 0.0; PY[0] = 0.0; PZ[0] = 0.0;
    VX[0] = 0.0; VY[0] = 0.0; VZ[0] = 0.0;
    MS[0] = SOLAR_MASS;
    set_body(1, 4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
        1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05,
        9.54791938424326609e-04);
    set_body(2, 8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
        -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05,
        2.85885980666130812e-04);
    set_body(3, 1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
        2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05,
        4.36624404335156298e-05);
    set_body(4, 1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
        2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05,
        5.15138902046611451e-05);
    double px = 0.0, py = 0.0, pz = 0.0;
    for (int i = 0; i < 5; i++) {
        px = px + VX[i] * MS[i];
        py = py + VY[i] * MS[i];
        pz = pz + VZ[i] * MS[i];
    }
    VX[0] = 0.0 - px / SOLAR_MASS;
    VY[0] = 0.0 - py / SOLAR_MASS;
    VZ[0] = 0.0 - pz / SOLAR_MASS;
}

static void advance(double dt) {
    for (int i = 0; i < 5; i++) {
        for (int j = i + 1; j < 5; j++) {
            const double dx = PX[i] - PX[j];
            const double dy = PY[i] - PY[j];
            const double dz = PZ[i] - PZ[j];
            const double d2 = dx * dx + dy * dy + dz * dz;
            const double mag = dt / (d2 * sqrt(d2));
            VX[i] = VX[i] - dx * MS[j] * mag;
            VY[i] = VY[i] - dy * MS[j] * mag;
            VZ[i] = VZ[i] - dz * MS[j] * mag;
            VX[j] = VX[j] + dx * MS[i] * mag;
            VY[j] = VY[j] + dy * MS[i] * mag;
            VZ[j] = VZ[j] + dz * MS[i] * mag;
        }
    }
    for (int k = 0; k < 5; k++) {
        PX[k] = PX[k] + dt * VX[k];
        PY[k] = PY[k] + dt * VY[k];
        PZ[k] = PZ[k] + dt * VZ[k];
    }
}

static double energy(void) {
    double e = 0.0;
    for (int i = 0; i < 5; i++) {
        e = e + 0.5 * MS[i] * (VX[i] * VX[i] + VY[i] * VY[i] + VZ[i] * VZ[i]);
        for (int j = i + 1; j < 5; j++) {
            const double dx = PX[i] - PX[j];
            const double dy = PY[i] - PY[j];
            const double dz = PZ[i] - PZ[j];
            e = e - MS[i] * MS[j] / sqrt(dx * dx + dy * dy + dz * dz);
        }
    }
    return e;
}

int main(void) {
    init();
    for (int64_t s = 0; s < STEPS; s++) {
        advance(0.01);
    }
    printf("%lld\n", (long long)(int64_t)(energy() * 1000000000.0));
    return 0;
}
