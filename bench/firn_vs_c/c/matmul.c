// Benchmark: i64 matrix multiplication 700x700 (naive i-j-k loops).
// Checksum = fold over all C entries: h = h * 1000003 + c (wrapping u64).
// Counterpart: bench/firn_vs_c/firn/matmul.fi
#include <stdio.h>
#include <stdint.h>
#include <stddef.h>

#define N 700

static int64_t MA[N * N], MB[N * N], MC[N * N];

int main(void) {
    for (size_t i = 0; i < N; i++) {
        for (size_t j = 0; j < N; j++) {
            MA[i * N + j] = (int64_t)((i * 31 + j * 17 + 3) % 101);
            MB[i * N + j] = (int64_t)((i * 13 + j * 7 + 5) % 97);
        }
    }
    for (size_t r = 0; r < N; r++) {
        for (size_t c = 0; c < N; c++) {
            int64_t s = 0;
            for (size_t k = 0; k < N; k++) {
                s = s + MA[r * N + k] * MB[k * N + c];
            }
            MC[r * N + c] = s;
        }
    }
    uint64_t h = 0;
    for (size_t d = 0; d < (size_t)N * N; d++) {
        h = h * 1000003u + (uint64_t)MC[d];
    }
    printf("%llu\n", (unsigned long long)h);
    return 0;
}
