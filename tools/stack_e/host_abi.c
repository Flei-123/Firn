/* SPDX-License-Identifier: MPL-2.0
 * tools/stack_e/host_abi.c -- one C host for every ABI (mingw/Win64 under Wine, System V x86-64, AAPCS64 under qemu):
 * loads a library built from lib_abi.fi and calls exports with floats and with more arguments than registers.
 * argv[1] = library. Prints "host_abi: all ok" if every result matches the formulas below. */
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
#define LOAD(p) ((void *)LoadLibraryA(p))
#define SYM(h, n) ((void *)GetProcAddress((HMODULE)(h), n))
#else
#include <dlfcn.h>
#define LOAD(p) dlopen(p, RTLD_NOW)
#define SYM(h, n) dlsym(h, n)
#endif
static int bad = 0;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL " __VA_ARGS__); printf("\n"); bad++; } } while (0)
int main(int argc, char **argv) {
    if (argc < 2) return 2;
    void *h = LOAD(argv[1]);
    if (!h) { printf("FAIL load %s\n", argv[1]); return 1; }
    double (*fadd)(double, double) = SYM(h, "abi_fadd");
    float (*fneg32)(float, float) = SYM(h, "abi_fneg32");
    double (*mix4)(int32_t, double, int64_t, float) = SYM(h, "abi_mix4");
    int64_t (*int8)(int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t) = SYM(h, "abi_int8");
    double (*dbl10)(double, double, double, double, double, double, double, double, double, double) = SYM(h, "abi_dbl10");
    double (*mix14)(int32_t, double, int64_t, float, uint8_t, double, int16_t, float, uint64_t, double, int32_t, double, int64_t, float) = SYM(h, "abi_mix14");
    double (*int9f)(int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t) = SYM(h, "abi_int9_f");
    int64_t (*fcount)(double, double, double, double, double) = SYM(h, "abi_fcount");
    CHECK(fadd && fneg32 && mix4 && int8 && dbl10 && mix14 && int9f && fcount, "symbols");
    if (bad) return 1;
    CHECK(fadd(1.5, 2.25) == 3.75, "fadd %f", fadd(1.5, 2.25));
    CHECK(fneg32(1.5f, 4.0f) == 2.5f, "fneg32 %f", (double)fneg32(1.5f, 4.0f));
    CHECK(mix4(-3, 0.5, 7, 2.5f) == -3 + 1.0 + 21.0 + 10.0, "mix4 %f", mix4(-3, 0.5, 7, 2.5f));
    CHECK(int8(1, 2, 3, 4, 5, 6, 7, 8) == 1 + 4 + 9 + 16 + 25 + 36 + 49 + 64, "int8 %lld", (long long)int8(1, 2, 3, 4, 5, 6, 7, 8));
    CHECK(int8(-1, -2, -3, -4, -5, -6, -7, -8) == -204, "int8 neg");
    CHECK(dbl10(1, 2, 3, 4, 5, 6, 7, 8, 9, 10) == 385.0, "dbl10 %f", dbl10(1, 2, 3, 4, 5, 6, 7, 8, 9, 10));
    CHECK(dbl10(0.5, 0, 0, 0, 0, 0, 0, 0, 0, 0.25) == 3.0, "dbl10 b");
    double want = 1 + 2 * 2.0 + 3 * 3.0 + 4 * 1.5 + 5 * 5.0 + 6 * 0.5 + 7 * -7.0 + 8 * 2.0 + 9 * 9.0 + 10 * 0.25 + 11 * -11.0 + 12 * 12.5 + 13 * 13.0 + 14 * 0.75;
    double got = mix14(1, 2.0, 3, 1.5f, 5, 0.5, -7, 2.0f, 9, 0.25, -11, 12.5, 13, 0.75f);
    CHECK(got == want, "mix14 got %f want %f", got, want);
    CHECK(int9f(1, 2, 3, 4, 5, 6, 7, 8, 9) == 22.5, "int9f %f", int9f(1, 2, 3, 4, 5, 6, 7, 8, 9));
    CHECK(fcount(1, -1, 1, -1, 1) == 1 + 4 + 16, "fcount %lld", (long long)fcount(1, -1, 1, -1, 1));
    /* many calls: nothing may drift (callee-saved registers of the host survive) */
    double acc = 0;
    for (int i = 0; i < 1000; i++) acc += fadd((double)i, 0.5) + mix4(i, 1.0, i, 1.0f);
    CHECK(acc == 499500.0 + 500.0 + (499500.0 + 3 * 499500.0 + 1000 * 2.0 + 1000 * 4.0), "loop %f", acc);
    if (!bad) printf("host_abi: all ok\n");
    return bad ? 1 : 0;
}
