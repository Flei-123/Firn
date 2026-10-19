// Word count: build text of 3M LCG words (1-6 letters from a-d, space separated),
// then 3 counting passes into a 16384-slot open-addressing table keyed by the
// FNV-1a hash of each word. Checksum over the table in slot order.
// Firn counterpart: bench/firn_vs_c/firn/wordcount.fi
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

static inline u64 lcg(u64 s) { return s * 6364136223846793005ULL + 1442695040888963407ULL; }

static u64 KEYS[16384];
static u64 COUNTS[16384];

static void add_word(u64 h) {
    size_t slot = (size_t)((h >> 20) & 16383);
    for (;;) {
        u64 k = KEYS[slot];
        if (k == h) { COUNTS[slot]++; return; }
        if (k == 0) { KEYS[slot] = h; COUNTS[slot] = 1; return; }
        slot = (slot + 1) & 16383;
    }
}

int main(void) {
    size_t nwords = 3000000;
    size_t cap = nwords * 7 + 8;
    u8 *text = (u8 *)calloc(cap, 1);
    u64 s = 99;
    size_t at = 0;
    for (size_t w = 0; w < nwords; w++) {
        s = lcg(s);
        u64 len = 1 + ((s >> 59) % 6);
        for (u64 c = 0; c < len; c++) {
            s = lcg(s);
            text[at++] = (u8)(97 + (s >> 62));
        }
        text[at++] = 32;
    }
    for (u32 pass = 0; pass < 3; pass++) {
        u64 h = 14695981039346656037ULL;
        for (size_t i = 0; i < at; i++) {
            u8 b = text[i];
            if (b == 32) {
                add_word(h);
                h = 14695981039346656037ULL;
            } else {
                h = (h ^ (u64)b) * 1099511628211ULL;
            }
        }
    }
    u64 acc = 0, distinct = 0;
    for (size_t j = 0; j < 16384; j++) {
        if (KEYS[j] != 0) distinct++;
        acc = acc * 31 + (KEYS[j] ^ (COUNTS[j] * 1000003ULL));
    }
    printf("%llu\n", (unsigned long long)(acc * 1000003ULL + distinct));
    return 0;
}
