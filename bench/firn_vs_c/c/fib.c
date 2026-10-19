// Benchmark: naive recursive Fibonacci, fib(40).
// Counterpart: bench/firn_vs_c/firn/fib.fi
#include <stdio.h>
#include <stdint.h>

static uint64_t fib(uint32_t n) {
    if (n < 2) {
        return (uint64_t)n;
    }
    return fib(n - 1) + fib(n - 2);
}

int main(void) {
    printf("%llu\n", (unsigned long long)fib(40));
    return 0;
}
