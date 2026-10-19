// Open-addressing (linear probing) hash map u64 -> u64, 4M slots of (key, value)
// pairs (64 MiB). 2M inserts, 2M hit lookups, 2M (almost always) miss lookups.
// Key 0 marks an empty slot; keys are forced non-zero with `| 1`.
// Firn counterpart: bench/firn_vs_c/firn/hashmap.fi
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

static inline u64 lcg(u64 s) { return s * 6364136223846793005ULL + 1442695040888963407ULL; }

static inline size_t slot_of(u64 k) {
    return (size_t)((k * 0x9E3779B97F4A7C15ULL) >> 42);
}

static void put(u64 *t, u64 k, u64 v) {
    size_t s = slot_of(k);
    for (;;) {
        u64 cur = t[s * 2];
        if (cur == 0 || cur == k) {
            t[s * 2] = k;
            t[s * 2 + 1] = v;
            return;
        }
        s = (s + 1) & 4194303;
    }
}

/* returns value, or 0 when the key is absent (values are never 0: they are index + 1) */
static u64 get(u64 *t, u64 k) {
    size_t s = slot_of(k);
    for (;;) {
        u64 cur = t[s * 2];
        if (cur == k) return t[s * 2 + 1];
        if (cur == 0) return 0;
        s = (s + 1) & 4194303;
    }
}

int main(void) {
    u64 n = 2000000;
    u64 *t = (u64 *)calloc(4194304, 16);
    u64 s = 12345;
    for (u64 i = 0; i < n; i++) {
        s = lcg(s);
        put(t, s | 1, i + 1);
    }
    u64 sum = 0;
    s = 12345;
    for (u64 i = 0; i < n; i++) {
        s = lcg(s);
        sum += get(t, s | 1);
    }
    u64 s2 = 777, misses = 0;
    for (u64 i = 0; i < n; i++) {
        s2 = lcg(s2);
        u64 v = get(t, s2 | 1);
        if (v == 0) misses++;
        else sum += v * 3;
    }
    printf("%llu\n", (unsigned long long)(sum * 1000003ULL + misses));
    return 0;
}
