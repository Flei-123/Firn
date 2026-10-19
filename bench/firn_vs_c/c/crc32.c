// Table-driven CRC-32 (poly 0xEDB88320) over a 64 MiB LCG buffer, 3 chained passes.
// Firn counterpart: bench/firn_vs_c/firn/crc32.fi
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

static inline u64 lcg(u64 s) { return s * 6364136223846793005ULL + 1442695040888963407ULL; }

static u32 TABLE[256];

static void make_table(void) {
    for (u32 i = 0; i < 256; i++) {
        u32 c = i;
        for (u32 k = 0; k < 8; k++) {
            if (c & 1) c = 0xEDB88320u ^ (c >> 1);
            else c = c >> 1;
        }
        TABLE[i] = c;
    }
}

int main(void) {
    make_table();
    size_t n = 67108864;
    u8 *buf = (u8 *)calloc(n, 1);
    u64 s = 1;
    for (size_t i = 0; i < n; i++) {
        s = lcg(s);
        buf[i] = (u8)(s >> 56);
    }
    u32 crc = 0xFFFFFFFFu;
    for (u32 pass = 0; pass < 3; pass++) {
        for (size_t i = 0; i < n; i++) {
            u32 idx = (crc ^ (u32)buf[i]) & 255;
            crc = TABLE[idx] ^ (crc >> 8);
        }
    }
    printf("%llu\n", (unsigned long long)(crc ^ 0xFFFFFFFFu));
    return 0;
}
