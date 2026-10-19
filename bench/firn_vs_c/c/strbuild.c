// Byte buffer with doubling growth: append decimal numbers + ',' (1M pieces per
// round, 3 rounds, buffer restarts at 16 bytes each round). Checksum over the bytes.
// Firn counterpart: bench/firn_vs_c/firn/strbuild.fi
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

static inline u64 lcg(u64 s) { return s * 6364136223846793005ULL + 1442695040888963407ULL; }
#pragma GCC optimize("no-tree-loop-distribute-patterns")

static u8 *G_BUF;
static size_t G_LEN, G_CAP;

static void grow(void) {
    size_t ncap = G_CAP * 2;
    u8 *nbuf = (u8 *)malloc(ncap);
    /* copy used part, 8 bytes at a time (capacities are multiples of 8) */
    size_t words = (G_LEN + 7) / 8;
    u64 *d = (u64 *)nbuf, *s = (u64 *)G_BUF;
    for (size_t j = 0; j < words; j++) d[j] = s[j];
    free(G_BUF);
    G_BUF = nbuf;
    G_CAP = ncap;
}

static inline void push_byte(u8 b) {
    if (G_LEN == G_CAP) grow();
    G_BUF[G_LEN++] = b;
}

static void push_dec(u64 v) {
    u8 tmp[24];
    size_t n = 0;
    u64 x = v;
    if (x == 0) { tmp[0] = 48; n = 1; }
    while (x > 0) { tmp[n++] = (u8)(x % 10) + 48; x /= 10; }
    while (n > 0) { n--; push_byte(tmp[n]); }
}

int main(void) {
    u64 total = 0, s = 7;
    for (u32 round = 0; round < 3; round++) {
        G_CAP = 16;
        G_LEN = 0;
        G_BUF = (u8 *)malloc(G_CAP);
        for (u32 i = 0; i < 1000000; i++) {
            s = lcg(s);
            push_dec(s >> 40);
            push_byte(44);
        }
        u64 h = 14695981039346656037ULL;
        for (size_t k = 0; k < G_LEN; k++) h = (h ^ (u64)G_BUF[k]) * 1099511628211ULL;
        total = total * 1000003ULL + (h ^ (u64)G_LEN);
        free(G_BUF);
    }
    printf("%llu\n", (unsigned long long)total);
    return 0;
}
