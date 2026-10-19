// Benchmark: bit operations, 200,000,000 steps: xorshift64, SWAR popcount,
// rotate-left. Checksum = wrapping mix of popcounts and rotated values.
// Counterpart: bench/firn_vs_c/firn/bitops.fi
#include <stdio.h>
#include <stdint.h>

#define STEPS 200000000ULL

static uint64_t popcount(uint64_t v) {
    uint64_t x = v;
    x = x - ((x >> 1) & 0x5555555555555555ULL);
    x = (x & 0x3333333333333333ULL) + ((x >> 2) & 0x3333333333333333ULL);
    x = (x + (x >> 4)) & 0x0f0f0f0f0f0f0f0fULL;
    return (x * 0x0101010101010101ULL) >> 56;
}

static uint64_t rotl(uint64_t v, uint64_t k) {
    return (v << k) | (v >> (64 - k));
}

int main(void) {
    uint64_t x = 88172645463325252ULL;
    uint64_t acc = 0;
    for (uint64_t i = 0; i < STEPS; i++) {
        x = x ^ (x << 13);
        x = x ^ (x >> 7);
        x = x ^ (x << 17);
        acc = acc + popcount(x);
        acc = acc ^ rotl(x, (i & 63) | 1);
    }
    printf("%llu\n", (unsigned long long)acc);
    return 0;
}
