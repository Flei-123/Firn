// Benchmark: f64 loop with sqrt/mul/add/div, 100,000,000 iterations
// (dependency chain through x). Output = round(sum) as i64.
// Counterpart: bench/firn_vs_c/firn/floatloop.fi
#include <stdio.h>
#include <stdint.h>
#include <math.h>

#define STEPS 100000000LL

int main(void) {
    double x = 1.0;
    double s = 0.0;
    for (int64_t i = 0; i < STEPS; i++) {
        const double t = (double)i * 0.0000001 + 1.0;
        x = sqrt(x * x * 0.5 + t);
        s = s + x * 0.5 + 1.0 / (x + t);
    }
    printf("%lld\n", (long long)(int64_t)(s + 0.5));
    return 0;
}
