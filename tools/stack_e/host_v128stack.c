// tools/stack_e/host_v128stack.c -- r570: calls the Firn exports with more than eight vector arguments. gcc places them
// (xmm0-7 / v0-v7, the rest on the stack at 16 octets, 16 aligned); Firn has to agree, on x86-64 and aarch64.
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef float v4 __attribute__((vector_size(16)));
typedef uint64_t (*f_t)(uint64_t out, uint64_t, v4, uint64_t, v4, uint64_t, v4, uint64_t, v4, uint64_t, v4, uint64_t, v4,
                        uint64_t, v4, uint64_t, v4, uint64_t, v4, double, v4, uint64_t);
typedef void (*s_t)(uint64_t out, uint64_t p);

int main(int argc, char **argv) {
#ifdef _WIN32
    HMODULE h = LoadLibraryA(argv[1]);
    if (!h) { printf("LoadLibrary failed\n"); return 2; }
    f_t f = (f_t)(void *)GetProcAddress(h, "se_v128_stack");
    s_t sum = (s_t)(void *)GetProcAddress(h, "se_v128_sum10");
#else
    void *h = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!h) { printf("dlopen failed: %s\n", dlerror()); return 2; }
    f_t f = (f_t)dlsym(h, "se_v128_stack");
    s_t sum = (s_t)dlsym(h, "se_v128_sum10");
#endif
    if (!f || !sum) { printf("missing symbol\n"); return 3; }
    int bad = 0;
    for (int round = 0; round < 50; round++) {
        v4 v[10];
        float raw[10][4] __attribute__((aligned(16)));
        for (int k = 0; k < 10; k++)
            for (int l = 0; l < 4; l++) raw[k][l] = (float)(round * 100 + k * 10 + l) + 0.25f;
        for (int k = 0; k < 10; k++) memcpy(&v[k], raw[k], 16);
        uint64_t out[32] __attribute__((aligned(16)));
        memset(out, 0, sizeof out);
        uint64_t i[10];
        for (int k = 0; k < 10; k++) i[k] = 1000003ull * (k + 1) + round;
        uint64_t r = f((uint64_t)(uintptr_t)out, i[0], v[0], i[1], v[1], i[2], v[2], i[3], v[3], i[4], v[4], i[5], v[5], i[6], v[6], i[7], v[7],
                       i[8], v[8], 7.0, v[9], i[9]);
        uint64_t want = 0;
        for (int k = 0; k < 10; k++) want += i[k];
        if (r != want) { printf("FAIL sum round %d: %llu != %llu\n", round, (unsigned long long)r, (unsigned long long)want); bad++; }
        for (int k = 0; k < 10; k++)
            if (memcmp(&out[2 * k], raw[k], 16)) { printf("FAIL vector %d round %d\n", k, round); bad++; }
        for (int k = 0; k < 10; k++)
            if (out[20 + k] != i[k]) { printf("FAIL int %d round %d\n", k, round); bad++; }
        if (out[30] != 7) { printf("FAIL double round %d\n", round); bad++; }
        float flat[10][4] __attribute__((aligned(16)));
        memcpy(flat, raw, sizeof raw);
        uint64_t sout[2] __attribute__((aligned(16)));
        sum((uint64_t)(uintptr_t)sout, (uint64_t)(uintptr_t)flat);
        float res[4];
        memcpy(res, sout, 16);
        for (int l = 0; l < 4; l++) {
            float w = 0;
            for (int k = 0; k < 10; k++) w += raw[k][l];
            if (res[l] != w && (res[l] - w > 0.01f || w - res[l] > 0.01f)) { printf("FAIL sum10 lane %d round %d\n", l, round); bad++; }
        }
    }
    printf(bad ? "FAIL host_v128stack (%d)\n" : "ok  host_v128stack\n", bad);
    return bad != 0;
}
