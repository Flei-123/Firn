// Benchmark: spectral norm, n = 1500, 10 power iterations (f64).
// Output = sqrt(vBv / vv) * 1e9 as i64.
// Counterpart: bench/firn_vs_c/firn/spectral_norm.fi
#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <math.h>

#define N 1500
#define OFF_U 0
#define OFF_V 1500
#define OFF_T 3000

static double BUF[4500];

static double eval_a(int64_t i, int64_t j) {
    return 1.0 / (double)((i + j) * (i + j + 1) / 2 + i + 1);
}

static void mul_av(size_t src, size_t dst) {
    for (size_t i = 0; i < N; i++) {
        double s = 0.0;
        for (size_t j = 0; j < N; j++) {
            s = s + eval_a((int64_t)i, (int64_t)j) * BUF[src + j];
        }
        BUF[dst + i] = s;
    }
}

static void mul_atv(size_t src, size_t dst) {
    for (size_t i = 0; i < N; i++) {
        double s = 0.0;
        for (size_t j = 0; j < N; j++) {
            s = s + eval_a((int64_t)j, (int64_t)i) * BUF[src + j];
        }
        BUF[dst + i] = s;
    }
}

static void mul_atav(size_t src, size_t dst) {
    mul_av(src, OFF_T);
    mul_atv(OFF_T, dst);
}

int main(void) {
    for (size_t i = 0; i < N; i++) BUF[OFF_U + i] = 1.0;
    for (int it = 0; it < 10; it++) {
        mul_atav(OFF_U, OFF_V);
        mul_atav(OFF_V, OFF_U);
    }
    double vbv = 0.0, vv = 0.0;
    for (size_t k = 0; k < N; k++) {
        vbv = vbv + BUF[OFF_U + k] * BUF[OFF_V + k];
        vv = vv + BUF[OFF_V + k] * BUF[OFF_V + k];
    }
    printf("%lld\n", (long long)(int64_t)(sqrt(vbv / vv) * 1000000000.0));
    return 0;
}
