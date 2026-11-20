#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <emmintrin.h>
#define COUNT 16384
#define REPS 400
int main(void) {
    float *src = malloc((COUNT + 8) * 4), *dst = malloc((COUNT + 8) * 4);
    for (size_t i = 0; i < COUNT + 8; i++) { src[i] = (float)((i * 37) % 251); dst[i] = 0; }
    __m128 two = _mm_set1_ps(2.0f), quarter = _mm_set1_ps(0.25f);
    for (int rep = 0; rep < REPS; rep++)
        for (size_t j = 1; j + 4 <= COUNT; j += 4) {
            __m128 l = _mm_loadu_ps(src + j - 1), m = _mm_loadu_ps(src + j), r = _mm_loadu_ps(src + j + 1);
            _mm_storeu_ps(dst + j, _mm_mul_ps(_mm_add_ps(_mm_add_ps(l, _mm_mul_ps(m, two)), r), quarter));
        }
    uint64_t sum = 0;
    for (size_t i = 1; i < COUNT; i++) sum += (uint64_t)(dst[i] * 4.0f);
    printf("%llu\n", (unsigned long long)sum);
    return 0;
}
