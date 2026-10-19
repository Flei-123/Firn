// Array-of-structs vs struct-of-arrays on 1M elements with 4 u64 fields (x, y, z, w).
// Both layouts hold the same data and run the same kernels:
//   A (touches all fields): x += y*3 + (z >> 1); w ^= x      -- 8 passes
//   B (touches x and w only): w += x                          -- 24 passes
// Checksum combines a fold of both layouts.
// Firn counterpart: bench/firn_vs_c/firn/soa_access.fi
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

static inline u64 lcg(u64 s) { return s * 6364136223846793005ULL + 1442695040888963407ULL; }

typedef struct { u64 x, y, z, w; } P;

int main(void) {
    size_t n = 1000000;
    P *aos = (P *)calloc(n, sizeof(P));
    u64 *sx = (u64 *)calloc(n, 8);
    u64 *sy = (u64 *)calloc(n, 8);
    u64 *sz = (u64 *)calloc(n, 8);
    u64 *sw = (u64 *)calloc(n, 8);
    u64 s = 5;
    for (size_t i = 0; i < n; i++) {
        s = lcg(s); u64 x = s >> 20;
        s = lcg(s); u64 y = s >> 20;
        s = lcg(s); u64 z = s >> 20;
        s = lcg(s); u64 w = s >> 20;
        aos[i].x = x; aos[i].y = y; aos[i].z = z; aos[i].w = w;
        sx[i] = x; sy[i] = y; sz[i] = z; sw[i] = w;
    }
    for (u32 pass = 0; pass < 8; pass++) {
        for (size_t i = 0; i < n; i++) {
            u64 nx = aos[i].x + aos[i].y * 3 + (aos[i].z >> 1);
            aos[i].x = nx;
            aos[i].w ^= nx;
        }
        for (size_t i = 0; i < n; i++) {
            u64 nx = sx[i] + sy[i] * 3 + (sz[i] >> 1);
            sx[i] = nx;
            sw[i] ^= nx;
        }
    }
    for (u32 pass = 0; pass < 24; pass++) {
        for (size_t i = 0; i < n; i++) aos[i].w += aos[i].x;
        for (size_t i = 0; i < n; i++) sw[i] += sx[i];
    }
    u64 fa = 0, fs = 0;
    for (size_t i = 0; i < n; i++) {
        fa = fa * 31 + (aos[i].x ^ aos[i].w);
        fs = fs * 31 + (sx[i] ^ sw[i]);
    }
    printf("%llu\n", (unsigned long long)(fa * 1000003ULL + fs));
    return 0;
}
