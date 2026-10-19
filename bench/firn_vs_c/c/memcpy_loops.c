// Hand-written byte / u64 copy and fill loops over 16 MiB buffers, 12 rounds.
// No libc memcpy/memset (loop-to-memcpy conversion is disabled below).
// Firn counterpart: bench/firn_vs_c/firn/memcpy_loops.fi
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

static inline u64 lcg(u64 s) { return s * 6364136223846793005ULL + 1442695040888963407ULL; }
#pragma GCC optimize("no-tree-loop-distribute-patterns")

int main(void) {
    size_t n = 16777216;
    size_t words = n / 8;
    u8 *a = (u8 *)calloc(n, 1);
    u8 *b = (u8 *)calloc(n, 1);
    u8 *c = (u8 *)calloc(n, 1);
    u64 *aw = (u64 *)a, *bw = (u64 *)b, *cw = (u64 *)c;
    u64 acc = 0;
    for (u64 r = 0; r < 12; r++) {
        for (size_t j = 0; j < words; j++) aw[j] = ((u64)j + r) * 0x9E3779B97F4A7C15ULL;
        for (size_t i = 0; i < n; i++) b[i] = a[i];
        for (size_t i = 0; i < n / 2; i++) a[i] = (u8)(((u64)i + r) & 255);
        for (size_t j = 0; j < words; j++) cw[j] = bw[j];
        for (size_t j = 0; j < words; j += 4099) acc = acc * 31 + cw[j];
    }
    for (size_t j = 0; j < words; j++) acc = acc * 31 + (aw[j] ^ cw[j]);
    printf("%llu\n", (unsigned long long)acc);
    return 0;
}
