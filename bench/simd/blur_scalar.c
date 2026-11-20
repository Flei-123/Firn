#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#define COUNT 16384
#define REPS 400
int main(void) {
    float *src = malloc((COUNT + 8) * 4), *dst = malloc((COUNT + 8) * 4);
    for (size_t i = 0; i < COUNT + 8; i++) { src[i] = (float)((i * 37) % 251); dst[i] = 0; }
    for (int rep = 0; rep < REPS; rep++)
        for (size_t j = 1; j + 4 <= COUNT; j++) dst[j] = (src[j - 1] + src[j] * 2.0f + src[j + 1]) * 0.25f;
    uint64_t sum = 0;
    for (size_t i = 1; i < COUNT; i++) sum += (uint64_t)(dst[i] * 4.0f);
    printf("%llu\n", (unsigned long long)sum);
    return 0;
}
