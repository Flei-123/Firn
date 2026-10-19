// Benchmark: fannkuch-redux, n = 10 (permutation flipping).
// Output = checksum * 1000 + max flips (i64).
// Counterpart: bench/firn_vs_c/firn/fannkuch.fi
#include <stdio.h>
#include <stdint.h>

#define N 10

static int32_t PERM[N], PERM1[N], COUNT[N];

int main(void) {
    for (size_t i = 0; i < N; i++) PERM1[i] = (int32_t)i;
    size_t r = N;
    int64_t perm_count = 0, checksum = 0, max_flips = 0;
    for (;;) {
        while (r != 1) {
            COUNT[r - 1] = (int32_t)r;
            r--;
        }
        for (size_t c = 0; c < N; c++) PERM[c] = PERM1[c];
        int64_t flips = 0;
        int32_t k = PERM[0];
        while (k != 0) {
            size_t lo = 0, hi = (size_t)k;
            while (lo < hi) {
                int32_t t = PERM[lo];
                PERM[lo] = PERM[hi];
                PERM[hi] = t;
                lo++;
                hi--;
            }
            flips++;
            k = PERM[0];
        }
        if (flips > max_flips) max_flips = flips;
        if (perm_count % 2 == 0) checksum += flips; else checksum -= flips;
        int more = 1;
        while (more) {
            if (r == N) {
                printf("%lld\n", (long long)(checksum * 1000 + max_flips));
                return 0;
            }
            int32_t p0 = PERM1[0];
            for (size_t m = 0; m < r; m++) PERM1[m] = PERM1[m + 1];
            PERM1[r] = p0;
            COUNT[r]--;
            if (COUNT[r] > 0) more = 0; else r++;
        }
        perm_count++;
    }
}
