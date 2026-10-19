// Benchmark: sieve of Eratosthenes up to 20,000,000; checksum = sum of primes.
// Counterpart: bench/firn_vs_c/firn/primes.fi
#include <stdio.h>
#include <stdint.h>

#define LIMIT 20000000UL

static uint8_t sieve[LIMIT + 1];

int main(void) {
    for (size_t i = 2; i * i <= LIMIT; i++) {
        if (sieve[i] == 0) {
            for (size_t j = i * i; j <= LIMIT; j += i) {
                sieve[j] = 1;
            }
        }
    }
    uint64_t sum = 0;
    for (size_t k = 2; k <= LIMIT; k++) {
        if (sieve[k] == 0) {
            sum += (uint64_t)k;
        }
    }
    printf("%llu\n", (unsigned long long)sum);
    return 0;
}
