#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <emmintrin.h>
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
        __m128 a0 = _mm_setzero_ps(), a1 = _mm_setzero_ps(), a2 = _mm_setzero_ps(), a3 = _mm_setzero_ps();
        for (size_t j = 0; j < COUNT; j += 16) {
            a0 = _mm_add_ps(a0, _mm_mul_ps(_mm_loadu_ps(a + j), _mm_loadu_ps(b + j)));
            a1 = _mm_add_ps(a1, _mm_mul_ps(_mm_loadu_ps(a + j + 4), _mm_loadu_ps(b + j + 4)));
            a2 = _mm_add_ps(a2, _mm_mul_ps(_mm_loadu_ps(a + j + 8), _mm_loadu_ps(b + j + 8)));
            a3 = _mm_add_ps(a3, _mm_mul_ps(_mm_loadu_ps(a + j + 12), _mm_loadu_ps(b + j + 12)));
        }
        __m128 s = _mm_add_ps(_mm_add_ps(a0, a2), _mm_add_ps(a1, a3));
        s = _mm_add_ps(s, _mm_shuffle_ps(s, s, 0xb1));
        s = _mm_add_ps(s, _mm_shuffle_ps(s, s, 0x4e));
        total += (int64_t)_mm_cvtss_f32(s) + 1000000;
    }
    printf("%llu\n", (unsigned long long)total);
    return 0;
}
