#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#define COUNT 16384
#define REPS 400
int main(void) {
    float *a = malloc(COUNT * 4), *b = malloc(COUNT * 4);
    for (size_t i = 0; i < COUNT; i++) {
        a[i] = (float)((i * 7) % 13) - 6.0f;
        b[i] = (float)((i * 5) % 11) - 5.0f;
    }
    int64_t total = 0;
    for (int rep = 0; rep < REPS; rep++) {
        float s = 0;
        for (size_t j = 0; j < COUNT; j++) s += a[j] * b[j];
        total += (int64_t)s + 1000000;
    }
    printf("%llu\n", (unsigned long long)total);
    return 0;
}
