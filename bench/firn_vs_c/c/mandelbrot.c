// Benchmark: Mandelbrot set, 800x800 pixels, max 500 iterations (f64).
// Checksum = total number of iterations over all pixels.
// Counterpart: bench/firn_vs_c/firn/mandelbrot.fi
#include <stdio.h>
#include <stdint.h>

#define SIZE 800
#define MAX_ITER 500

int main(void) {
    uint64_t total = 0;
    for (int64_t py = 0; py < SIZE; py++) {
        const double ci = -1.5 + 3.0 * (double)py / (double)SIZE;
        for (int64_t px = 0; px < SIZE; px++) {
            const double cr = -2.0 + 3.0 * (double)px / (double)SIZE;
            double zr = 0.0;
            double zi = 0.0;
            int64_t it = 0;
            while (it < MAX_ITER) {
                const double zr2 = zr * zr;
                const double zi2 = zi * zi;
                if (zr2 + zi2 > 4.0) {
                    break;
                }
                zi = 2.0 * zr * zi + ci;
                zr = zr2 - zi2 + cr;
                it++;
            }
            total += (uint64_t)it;
        }
    }
    printf("%llu\n", (unsigned long long)total);
    return 0;
}
