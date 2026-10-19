// FNV-1a 64-bit over a 64 MiB deterministic LCG buffer, 4 chained passes.
// Firn counterpart: bench/firn_vs_c/firn/fnv_hash.fi
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

static inline u64 lcg(u64 s) { return s * 6364136223846793005ULL + 1442695040888963407ULL; }

int main(void) {
    size_t n = 67108864;
    u8 *buf = (u8 *)calloc(n, 1);
    u64 s = 1;
    for (size_t i = 0; i < n; i++) {
        s = lcg(s);
        buf[i] = (u8)(s >> 56);
    }
    u64 h = 14695981039346656037ULL;
    for (u32 pass = 0; pass < 4; pass++) {
        for (size_t i = 0; i < n; i++) {
            h = (h ^ (u64)buf[i]) * 1099511628211ULL;
        }
    }
    printf("%llu\n", (unsigned long long)h);
    return 0;
}
