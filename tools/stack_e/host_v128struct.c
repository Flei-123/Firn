// tools/stack_e/host_v128struct.c -- r577: calls the exports of lib_v128struct.fi through the C ABI of the C compiler (gcc x86-64,
// aarch64-linux-gnu-gcc under qemu): vectors beyond the vector registers next to a struct that needs the entry thunk.
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef float v4 __attribute__((vector_size(16)));
typedef struct { double x, y; } D16;
typedef struct { double x, y, z; } D24;
typedef double (*f_nine)(uint64_t, v4, v4, v4, v4, v4, v4, v4, v4, v4, D24, int64_t);
typedef double (*f_ints)(uint64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, v4, v4, v4, v4, v4, v4, v4, v4, v4, D24, v4, int64_t);
typedef double (*f_regs)(uint64_t, double, double, double, double, double, double, double, D16, v4, int64_t);

static int bad = 0;
static float raw[10][4] __attribute__((aligned(16)));

static void check_vectors(const char *what, int round, uint64_t *out, int n) {
    for (int k = 0; k < n; k++)
        if (memcmp(&out[2 * k], raw[k], 16)) { printf("FAIL %s vector %d round %d\n", what, k, round); bad++; }
}

int main(int argc, char **argv) {
    void *h = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!h) { printf("dlopen failed: %s\n", dlerror()); return 2; }
    f_nine nine = (f_nine)dlsym(h, "sv_nine_d24");
    f_ints ints = (f_ints)dlsym(h, "sv_ints_align");
    f_regs regs = (f_regs)dlsym(h, "sv_regs_run_out");
    if (!nine || !ints || !regs) { printf("missing symbol\n"); return 3; }
    for (int round = 0; round < 50; round++) {
        v4 v[10];
        for (int k = 0; k < 10; k++)
            for (int l = 0; l < 4; l++) raw[k][l] = (float)(round * 100 + k * 10 + l) + 0.25f;
        for (int k = 0; k < 10; k++) memcpy(&v[k], raw[k], 16);
        uint64_t out[32] __attribute__((aligned(16)));
        D24 p = { 1.5 + round, 2.5, 3.5 };
        D16 q = { 4.5 + round, 5.5 };
        int64_t k = 7 + round;

        memset(out, 0, sizeof out);
        double r = nine((uint64_t)(uintptr_t)out, v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], p, k);
        double want = p.x + p.y * 2.0 + p.z * 3.0 + k * 100.0;
        if (r != want) { printf("FAIL nine_d24 result round %d: %f != %f\n", round, r, want); bad++; }
        check_vectors("nine_d24", round, out, 9);

        memset(out, 0, sizeof out);
        int64_t i[6];
        for (int j = 0; j < 6; j++) i[j] = 11 * (j + 1) + round;
        r = ints((uint64_t)(uintptr_t)out, i[0], i[1], i[2], i[3], i[4], i[5], v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], p, v[9], k);
        want = p.x + p.y * 2.0 + p.z * 3.0 + (double)(i[0] + i[1] * 2 + i[2] * 3 + i[3] * 4 + i[4] * 5 + i[5] * 6 + k);
        if (r != want) { printf("FAIL ints_align result round %d: %f != %f\n", round, r, want); bad++; }
        check_vectors("ints_align", round, out, 10);

        memset(out, 0, sizeof out);
        r = regs((uint64_t)(uintptr_t)out, 1, 2, 3, 4, 5, 6, 7, q, v[3], k);
        want = 1 + 2 * 2.0 + 3 * 3.0 + 4 * 4.0 + 5 * 5.0 + 6 * 6.0 + 7 * 7.0 + q.x * 8.0 + q.y * 9.0 + k * 100.0;
        if (r != want) { printf("FAIL regs_run_out result round %d: %f != %f\n", round, r, want); bad++; }
        if (memcmp(out, raw[3], 16)) { printf("FAIL regs_run_out vector round %d\n", round); bad++; }
    }
    printf(bad ? "FAIL host_v128struct (%d)\n" : "ok  host_v128struct\n", bad);
    return bad != 0;
}
