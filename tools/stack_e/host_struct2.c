/* SPDX-License-Identifier: MPL-2.0
 * tools/stack_e/host_struct2.c -- structs by value in Linux .so exports (r558), see lib_struct2.fi. argv[1] = library.
 * Built with gcc (System V x86-64) and aarch64-linux-gnu-gcc (AAPCS64, under qemu): the C compiler's own ABI is the reference.
 * Prints "host_struct2: all ok". */
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
typedef struct { float x, y, z; } F12;
typedef struct { float x, y, z, w; } F16;
typedef struct { double x, y; } D16;
typedef struct { double x, y, z; } D24;
typedef struct { double x, y, z, w; } D32;
typedef struct { double a; float b; } MD;
typedef struct { int32_t a; float b; double c; } IF;
typedef struct { int64_t a, b; } S16;
typedef struct { int32_t a, b, c; } S12;
typedef struct { int64_t a, b, c; } B24;
typedef struct { float x, y; } F8;
typedef struct { float v[2]; } A2;
static int bad = 0;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL " __VA_ARGS__); printf("\n"); bad++; } } while (0)
#define FN(ret, name, args) ret (*name) args = dlsym(h, "t_" #name); CHECK(name, "symbol t_" #name)
int main(int argc, char **argv) {
    void *h = argc > 1 ? dlopen(argv[1], RTLD_NOW) : 0;
    if (!h) { printf("FAIL load %s\n", argc > 1 ? argv[1] : ""); return 1; }
    FN(double, f12, (F12, double)); FN(double, f16, (F16, int64_t)); FN(double, d24, (D24, int64_t)); FN(double, d32, (D32, double));
    FN(double, md, (MD, int64_t)); FN(double, ifm, (IF, int64_t)); FN(double, a2, (A2, int64_t));
    FN(int64_t, fit5, (int64_t, int64_t, int64_t, int64_t, int64_t, S16, int64_t));
    FN(int64_t, fit7, (int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, S16, int64_t));
    FN(double, fitx, (double, double, double, double, double, double, double, D16, double));
    FN(int64_t, fit_mix, (int64_t, int64_t, int64_t, int64_t, int64_t, S12, S12, int64_t));
    FN(double, hfa_over, (D24, D24, double, D16, double)); FN(double, hfa_f, (F16, F16, F12, F8, float));
    FN(double, many, (int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, S12, F8, B24, D16, int64_t, F12, MD, IF));
    FN(F12, ret_f12, (float, float, float)); FN(F16, ret_f16, (float, float, float, float)); FN(D24, ret_d24, (double, double, double));
    FN(D32, ret_d32, (double, double, double, double)); FN(MD, ret_md, (double, float)); FN(IF, ret_if, (int32_t, float, double));
    FN(F8, ret_f8, (float, float)); FN(A2, ret_a2, (float, float));
    FN(F12, scale_f12, (F12, float)); FN(D24, scale_d24, (D24, double)); FN(B24, add_b24, (B24, S16, int64_t));
    FN(IF, swap_md, (MD, IF, int64_t)); FN(MD, swap_if, (IF, double)); FN(S16, stackret, (int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, int64_t, S12));
    if (bad) return 1;
    for (int64_t k = 0; k < 3; k++) {
        F12 a = {1.5f, 2.5f, 4.0f}; CHECK(f12(a, (double)k) == 1.5 + 5.0 + 12.0 + k * 100.0, "f12 %f", f12(a, (double)k));
        F16 b = {1.0f, 2.0f, 3.0f, 0.5f}; CHECK(f16(b, k) == 1 + 4 + 9 + 2.0 + k * 100.0, "f16 %f", f16(b, k));
        D24 c = {0.5, 1.5, 2.5}; CHECK(d24(c, k) == 0.5 + 3.0 + 7.5 + k * 100.0, "d24 %f", d24(c, k));
        D32 d = {0.5, 1.5, 2.5, 3.5}; CHECK(d32(d, (double)k) == 0.5 + 3.0 + 7.5 + 14.0 + k * 100.0, "d32 %f", d32(d, (double)k));
        MD e = {-1.25, 3.5f}; CHECK(md(e, k) == -1.25 + 7.0 + k * 100.0, "md %f", md(e, k));
        IF f = {-7, 0.25f, 2.5}; CHECK(ifm(f, k) == -7 + 0.5 + 7.5 + k * 100.0, "ifm %f", ifm(f, k));
        A2 g = {{1.5f, 2.25f}}; CHECK(a2(g, k) == 1.5 + 4.5 + k * 100.0, "a2 %f", a2(g, k));
        S16 s = {-3, 4}; CHECK(fit5(1, 2, 3, 4, 5, s, 6 + k) == 1 + 4 + 9 + 16 + 25 + 5 * 6 + (6 + k) * 7, "fit5 %lld", (long long)fit5(1, 2, 3, 4, 5, s, 6 + k));
        CHECK(fit7(1, 2, 3, 4, 5, 6, 7, s, 8 + k) == 1 + 4 + 9 + 16 + 25 + 36 + 49 + 5 * 8 + (8 + k) * 9, "fit7 %lld", (long long)fit7(1, 2, 3, 4, 5, 6, 7, s, 8 + k));
        D16 dd = {0.5, 1.5}; double w = 1 + 4 + 9 + 16 + 25 + 36 + 49 + 3.5 * 8 + (8 + k) * 9;
        CHECK(fitx(1, 2, 3, 4, 5, 6, 7, dd, 8 + k) == w, "fitx %f want %f", fitx(1, 2, 3, 4, 5, 6, 7, dd, 8 + k), w);
        S12 p = {1, 2, 3}, q = {4, 5, 6};
        CHECK(fit_mix(1, 2, 3, 4, 5, p, q, 9 + k) == 1 + 4 + 9 + 16 + 25 + 14 * 6 + 32 * 7 + (9 + k) * 8, "fit_mix %lld", (long long)fit_mix(1, 2, 3, 4, 5, p, q, 9 + k));
        D24 h1 = {1, 2, 3}, h2 = {4, 5, 6}; D16 h3 = {7, 8}; double hw = 14 + 32 * 4 + 5.0 * 5 + 23.0 * 6 + (9 + k) * 7;
        CHECK(hfa_over(h1, h2, 5.0, h3, 9 + k) == hw, "hfa_over %f want %f", hfa_over(h1, h2, 5.0, h3, 9 + k), hw);
        F16 f1 = {1, 2, 3, 4}, f2 = {5, 6, 7, 8}; F12 f3 = {1, 2, 3}; F8 f4 = {4, 5}; double fw = 30 + 70 * 5 + 14 * 6 + 14 * 7 + (9 + k) * 8;
        CHECK(hfa_f(f1, f2, f3, f4, 9 + k) == fw, "hfa_f %f want %f", hfa_f(f1, f2, f3, f4, 9 + k), fw);
        S12 m1 = {1, 2, 3}; F8 m2 = {0.5f, 0.25f}; B24 m3 = {1, 2, 3}; D16 m4 = {0.5, 1.5}; F12 m5 = {1, 2, 3}; MD m6 = {1.5, 2.5f}; IF m7 = {3, 1.5f, 2.0};
        double mw = 1 + 4 + 9 + 16 + 25 + 36 + 14 * 7 + 1.0 * 8 + 14 * 9 + 3.5 * 10 + 11 * 11.0 + 14 * 12 + 6.5 * 13 + (3 + 3.0 + 6.0) * 14;
        double mg = many(1, 2, 3, 4, 5, 6, m1, m2, m3, m4, 11, m5, m6, m7);
        CHECK(mg == mw, "many %f want %f", mg, mw);
    }
    for (int64_t k = 0; k < 3; k++) {
        F12 a = ret_f12(1.5f, 2.5f, (float)(3 + k)); CHECK(a.x == 1.5f && a.y == 2.5f && a.z == (float)(3 + k), "ret_f12 %f %f %f", a.x, a.y, a.z);
        F16 b = ret_f16(1, 2, 3, (float)(4 + k)); CHECK(b.x == 1 && b.y == 2 && b.z == 3 && b.w == (float)(4 + k), "ret_f16 %f %f %f %f", b.x, b.y, b.z, b.w);
        D24 c = ret_d24(0.5, 1.5, 2.5 + k); CHECK(c.x == 0.5 && c.y == 1.5 && c.z == 2.5 + k, "ret_d24 %f %f %f", c.x, c.y, c.z);
        D32 d = ret_d32(0.5, 1.5, 2.5, 3.5 + k); CHECK(d.x == 0.5 && d.y == 1.5 && d.z == 2.5 && d.w == 3.5 + k, "ret_d32 %f %f %f %f", d.x, d.y, d.z, d.w);
        MD e = ret_md(-2.5, (float)(1 + k)); CHECK(e.a == -2.5 && e.b == (float)(1 + k), "ret_md %f %f", e.a, e.b);
        IF f = ret_if(-9, 0.75f, 6.5 + k); CHECK(f.a == -9 && f.b == 0.75f && f.c == 6.5 + k, "ret_if %d %f %f", f.a, f.b, f.c);
        F8 g = ret_f8(1.5f, (float)(2 + k)); CHECK(g.x == 1.5f && g.y == (float)(2 + k), "ret_f8");
        A2 i = ret_a2(1.5f, (float)(2 + k)); CHECK(i.v[0] == 1.5f && i.v[1] == (float)(2 + k), "ret_a2");
        F12 sa = {1, 2, 3}; F12 sr = scale_f12(sa, (float)(2 + k)); CHECK(sr.x == 2.0f + k && sr.y == 4.0f + 2 * k && sr.z == 6.0f + 3 * k, "scale_f12 %f %f %f", sr.x, sr.y, sr.z);
        D24 sd = {1, 2, 3}; D24 sq = scale_d24(sd, 2.0 + k); CHECK(sq.x == 2.0 + k && sq.y == 4.0 + 2 * k && sq.z == 6.0 + 3 * k, "scale_d24");
        B24 ba = {10, 20, 30}; S16 bb = {1, 2}; B24 br = add_b24(ba, bb, k + 2); CHECK(br.a == 10 + (k + 2) && br.b == 20 + 2 * (k + 2) && br.c == 30 + k + 2, "add_b24 %lld %lld %lld", (long long)br.a, (long long)br.b, (long long)br.c);
        MD sm = {1.5, 2.5f}; IF si = {0, 0.0f, 4.0}; IF sx = swap_md(sm, si, 7 + k); CHECK(sx.a == 7 + k && sx.b == 2.5f && sx.c == 5.5, "swap_md %d %f %f", sx.a, sx.b, sx.c);
        IF sj = {1, 1.5f, 2.0}; MD sy = swap_if(sj, 3.0 + k); CHECK(sy.a == 2.0 * (3.0 + k) && sy.b == 1.5f, "swap_if %f %f", sy.a, sy.b);
        S12 sk = {1, 2, 3}; S16 sz = stackret(1, 2, 3, 4, 5, 6, 7 + k, sk); CHECK(sz.a == 28 + k && sz.b == 6, "stackret %lld %lld", (long long)sz.a, (long long)sz.b);
    }
    if (!bad) printf("host_struct2: all ok\n");
    return bad ? 1 : 0;
}
