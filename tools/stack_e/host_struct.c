/* SPDX-License-Identifier: MPL-2.0
 * tools/stack_e/host_struct.c -- structs by value in exports (r506). One C host for every ABI (mingw Win64 under Wine,
 * System V x86-64, AAPCS64 under qemu): loads a library built from lib_struct.fi and passes / returns structs the way the
 * C compiler does. argv[1] = library. Prints "host_struct: all ok" if every result matches. */
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
typedef struct { uint8_t a; } S1;
typedef struct { uint8_t a, b; } S2;
typedef struct { uint8_t a, b, c; } S3;
typedef struct { int16_t a, b; } S4;
typedef struct { uint8_t a[5]; } S5;
typedef struct { int16_t a, b, c; } S6;
typedef struct { uint8_t a[7]; } S7;
typedef struct { int32_t a, b; } S8;
typedef struct { float x, y; } F8;
typedef struct { double x; } D8;
typedef struct { float x; } F4;
typedef struct { int32_t a, b, c; } S12;
typedef struct { int64_t a, b; } S16;
typedef struct { double x, y; } D16;
typedef struct { int64_t a; double y; } M16;
typedef struct { int64_t a, b, c; } B24;
static int bad = 0;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL " __VA_ARGS__); printf("\n"); bad++; } } while (0)
#define NEED(v, name) do { v = SYM(h, name); CHECK(v, "symbol %s", name); } while (0)
int main(int argc, char **argv) {
    if (argc < 2) return 2;
    void *h = LOAD(argv[1]);
    if (!h) { printf("FAIL load %s\n", argv[1]); return 1; }
    int64_t (*s1)(S1, int64_t), (*s2)(S2, int64_t), (*s3)(S3, int64_t), (*s4)(S4, int64_t), (*s5)(S5, int64_t), (*s6)(S6, int64_t),
        (*s7)(S7, int64_t), (*s8)(S8, int64_t), (*s12)(S12, int64_t), (*s16)(S16, int64_t), (*b24)(B24, int64_t);
    double (*f8)(F8, double), (*d8)(D8, double), (*f4)(F4, double), (*d16)(D16, double), (*m16)(M16, int64_t);
    double (*mix)(int64_t, S8, double, F8, S16, double, B24, S3, D16, int64_t, S12, double);
    S1 (*r_s1)(int64_t); S3 (*r_s3)(int64_t); S6 (*r_s6)(int64_t); S7 (*r_s7)(int64_t); S8 (*r_s8)(int32_t, int32_t);
    F8 (*r_f8)(float, float); D8 (*r_d8)(double); S12 (*r_s12)(int32_t); S16 (*r_s16)(int64_t, int64_t); D16 (*r_d16)(double, double);
    B24 (*r_b24)(int64_t, int64_t, int64_t); S16 (*swap16)(S16); D16 (*scale)(D16, double);
    NEED(s1, "st_s1"); NEED(s2, "st_s2"); NEED(s3, "st_s3"); NEED(s4, "st_s4"); NEED(s5, "st_s5"); NEED(s6, "st_s6"); NEED(s7, "st_s7");
    NEED(s8, "st_s8"); NEED(f8, "st_f8"); NEED(d8, "st_d8"); NEED(f4, "st_f4"); NEED(s12, "st_s12"); NEED(s16, "st_s16");
    NEED(d16, "st_d16"); NEED(m16, "st_m16"); NEED(b24, "st_b24"); NEED(mix, "st_mix");
    NEED(r_s1, "st_ret_s1"); NEED(r_s3, "st_ret_s3"); NEED(r_s6, "st_ret_s6"); NEED(r_s7, "st_ret_s7"); NEED(r_s8, "st_ret_s8");
    NEED(r_f8, "st_ret_f8"); NEED(r_d8, "st_ret_d8"); NEED(r_s12, "st_ret_s12"); NEED(r_s16, "st_ret_s16"); NEED(r_d16, "st_ret_d16");
    NEED(r_b24, "st_ret_b24"); NEED(swap16, "st_swap16"); NEED(scale, "st_scale_d16");
    if (bad) return 1;
    /* arguments: the struct, then a scalar that must keep its position */
    for (int64_t k = 0; k < 3; k++) {
        S1 a1 = {(uint8_t)(200 + k)}; CHECK(s1(a1, k) == 200 + k + k * 100, "s1");
        S2 a2 = {250, (uint8_t)(7 + k)}; CHECK(s2(a2, k) == 250 + (7 + k) * 2 + k * 100, "s2");
        S3 a3 = {255, (uint8_t)(3 + k), 129}; CHECK(s3(a3, k) == 255 + (3 + k) * 2 + 129 * 3 + k * 100, "s3 %lld", (long long)s3(a3, k));
        S4 a4 = {-30000, (int16_t)(5 + k)}; CHECK(s4(a4, k) == -30000 + (5 + k) * 2 + k * 100, "s4");
        S5 a5 = {{9, 8, 7, 6, (uint8_t)(250 + k)}}; CHECK(s5(a5, k) == 9 + 16 + (250 + k) * 3 + k * 100, "s5 %lld", (long long)s5(a5, k));
        S6 a6 = {-1, 2, (int16_t)(-3000 + k)}; CHECK(s6(a6, k) == -1 + 4 + (-3000 + k) * 3 + k * 100, "s6 %lld", (long long)s6(a6, k));
        S7 a7 = {{1, 2, 3, 40, 5, 6, (uint8_t)(200 + k)}}; CHECK(s7(a7, k) == 1 + 80 + (200 + k) * 3 + k * 100, "s7 %lld", (long long)s7(a7, k));
        S8 a8 = {-100000, (int32_t)(7 + k)}; CHECK(s8(a8, k) == -100000 + (7 + k) * 2 + k * 100, "s8");
        F8 af8 = {1.5f, 2.25f}; CHECK(f8(af8, (double)k) == 1.5 + 4.5 + k * 100.0, "f8 %f", f8(af8, (double)k));
        D8 ad8 = {0.125 + k}; CHECK(d8(ad8, (double)k) == 0.125 + k + k * 100.0, "d8 %f", d8(ad8, (double)k));
        F4 af4 = {3.5f}; CHECK(f4(af4, (double)k) == 3.5 + k * 100.0, "f4 %f", f4(af4, (double)k));
        S12 a12 = {-1, 20, (int32_t)(300 + k)}; CHECK(s12(a12, k) == -1 + 40 + (300 + k) * 3 + k * 100, "s12 %lld", (long long)s12(a12, k));
        S16 a16 = {-5000000000LL, 6 + k}; CHECK(s16(a16, k) == -5000000000LL + (6 + k) * 2 + k * 100, "s16");
        D16 ad16 = {1.25, 2.5 + k}; CHECK(d16(ad16, (double)k) == 1.25 + (2.5 + k) * 2 + k * 100.0, "d16 %f", d16(ad16, (double)k));
        M16 am16 = {-7, 0.5 + k}; CHECK(m16(am16, k) == -7 + (0.5 + k) * 2 + k * 100.0, "m16 %f", m16(am16, k));
        B24 ab24 = {1000000000000LL, -2, 3 + k}; CHECK(b24(ab24, k) == 1000000000000LL - 4 + (3 + k) * 3 + k * 100, "b24 %lld", (long long)b24(ab24, k));
    }
    /* many aggregates and scalars mixed: every slot keeps its position, registers of both files, then the stack */
    {
        S8 p = {10, 20}; F8 q = {0.5f, 0.25f}; S16 c = {-3, 4}; B24 e = {5, 6, 7}; S3 f = {1, 2, 3}; D16 g = {0.5, 1.5}; S12 i = {1, 2, 3};
        double want = 1 + (10 + 40) + 2.0 * 3.0 + 0.5 * 4.0 + 0.25 * 5.0 + (-3 + 8) * 6.0 + 0.75 * 7.0 + (5 + 12 + 21) * 8.0 + (1 + 4 + 9) * 9.0 + (0.5 + 3.0) * 10.0 + 11 * 11.0 + (1 + 4 + 9) * 12.0 + 0.125 * 13.0;
        double got = mix(1, p, 2.0, q, c, 0.75, e, f, g, 11, i, 0.125);
        CHECK(got == want, "mix got %f want %f", got, want);
    }
    /* results */
    for (int64_t k = 0; k < 3; k++) {
        S1 x1 = r_s1(200 + k); CHECK(x1.a == 200 + k, "r_s1");
        S3 x3 = r_s3(100 + k); CHECK(x3.a == 100 + k && x3.b == 101 + k && x3.c == 102 + k, "r_s3 %d %d %d", x3.a, x3.b, x3.c);
        S6 x6 = r_s6(1000 + k); CHECK(x6.a == 1000 + k && x6.b == 1001 + k && x6.c == 1002 + k, "r_s6");
        S7 x7 = r_s7(50 + k); int ok7 = 1; for (int j = 0; j < 7; j++) ok7 &= x7.a[j] == 50 + k + j; CHECK(ok7, "r_s7");
        S8 x8 = r_s8(-70000, (int32_t)(8 + k)); CHECK(x8.a == -70000 && x8.b == 8 + k, "r_s8");
        F8 xf = r_f8(1.5f, (float)(2.5 + k)); CHECK(xf.x == 1.5f && xf.y == (float)(2.5 + k), "r_f8 %f %f", xf.x, xf.y);
        D8 xd = r_d8(6.25 + k); CHECK(xd.x == 6.25 + k, "r_d8 %f", xd.x);
        S12 x12 = r_s12((int32_t)(-5 + k)); CHECK(x12.a == -5 + k && x12.b == -4 + k && x12.c == -3 + k, "r_s12");
        S16 x16 = r_s16(-4000000000LL, 9 + k); CHECK(x16.a == -4000000000LL && x16.b == 9 + k, "r_s16");
        D16 xd16 = r_d16(0.5, 1.5 + k); CHECK(xd16.x == 0.5 && xd16.y == 1.5 + k, "r_d16 %f %f", xd16.x, xd16.y);
        B24 xb = r_b24(-1, 2000000000000LL, 3 + k); CHECK(xb.a == -1 && xb.b == 2000000000000LL && xb.c == 3 + k, "r_b24");
        S16 in = {11, 22}; S16 sw = swap16(in); CHECK(sw.a == 22 && sw.b == 11, "swap16");
        D16 din = {1.5, -2.5}; D16 sc = scale(din, 2.0 + k); CHECK(sc.x == 1.5 * (2.0 + k) && sc.y == -2.5 * (2.0 + k), "scale");
    }
    /* many calls: the host's callee-saved registers survive */
    int64_t acc = 0;
    for (int i = 0; i < 1000; i++) { S8 a = {i, 1}; S16 s = r_s16(i, 2); acc += s8(a, 0) + s.a; }
    CHECK(acc == 499500 * 2 + 2000, "loop %lld", (long long)acc);
    if (!bad) printf("host_struct: all ok\n");
    return bad ? 1 : 0;
}
