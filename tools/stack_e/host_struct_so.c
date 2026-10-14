/* SPDX-License-Identifier: MPL-2.0
 * tools/stack_e/host_struct_so.c -- the by-value struct shapes a Linux .so shares with C (see lib_struct_so.fi). argv[1] = library. */
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
typedef struct { uint8_t a, b, c; } S3;
typedef struct { int32_t a, b; } S8;
typedef struct { int32_t a, b, c; } S12;
typedef struct { int64_t a, b; } S16;
int main(int argc, char **argv) {
    void *h = argc > 1 ? dlopen(argv[1], RTLD_NOW) : 0;
    if (!h) { printf("FAIL load\n"); return 1; }
    int64_t (*s8)(S8, int64_t) = dlsym(h, "st_s8"); int64_t (*s12)(S12, int64_t) = dlsym(h, "st_s12");
    double (*mix)(int64_t, S16, double, S8) = dlsym(h, "st_s16_mix"); S3 (*r3)(int64_t) = dlsym(h, "st_ret_s3");
    S8 (*r8)(int32_t, int32_t) = dlsym(h, "st_ret_s8");
    if (!s8 || !s12 || !mix || !r3 || !r8) { printf("FAIL symbols\n"); return 1; }
    int bad = 0;
    S8 a = {-100000, 7}; if (s8(a, 3) != -100000 + 14 + 300) { printf("FAIL s8\n"); bad++; }
    S12 b = {-1, 20, 300}; if (s12(b, 2) != -1 + 40 + 900 + 200) { printf("FAIL s12\n"); bad++; }
    S16 c = {-3, 4}; S8 d = {5, 6};
    if (mix(1, c, 0.5, d) != 1 + (-3 + 8) * 2.0 + 1.5 + 44.0) { printf("FAIL mix %f\n", mix(1, c, 0.5, d)); bad++; }
    S3 e = r3(100); if (e.a != 100 || e.b != 101 || e.c != 102) { printf("FAIL r3\n"); bad++; }
    S8 f = r8(-70000, 9); if (f.a != -70000 || f.b != 9) { printf("FAIL r8\n"); bad++; }
    if (!bad) printf("host_struct_so: all ok\n");
    return bad ? 1 : 0;
}
