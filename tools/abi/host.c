/* SPDX-License-Identifier: GPL-2.0-only */
/* tools/abi/host.c -- THE GCC SIDE OF THE ABI CROSS CHECK (round 71).
 *
 * This file is a MEASURING INSTRUMENT, not a dependency: nothing in Firn
 * uses it. It exists so that "the Firn compiler agrees with itself" cannot
 * become "the Firn compiler is wrong in the same way twice".
 *
 * GCC follows System V AMD64. If Firn puts an `f32` somewhere else than
 * `xmm0`, or classifies `{ i64, f64 }` as two integer words, then the
 * numbers below come out wrong -- there is no third possibility.
 *
 * The Firn symbols carry the naming scheme `_F0.<name>` (DESIGN_GOALS 4),
 * which is why every declaration names its assembler symbol explicitly.
 */

#include <stdio.h>
#include <string.h>

typedef struct { float x, y; } V2;
typedef struct { float x, y, z, w; } V4;
typedef struct { double a, b; } D2;
typedef struct { long a; double x; } IntFloat;
typedef struct { double x; long a; } FloatInt;
typedef struct { int a; float x; } Mixed;

/* ---------------------------------------------------------- Firn callee */

extern float  abi_add_f32(float, float)            __asm__("_F0.abi_add_f32");
extern double abi_add_f64(double, double)          __asm__("_F0.abi_add_f64");
extern double abi_mix(int, float, long, double, int, float, long, double)
                                                   __asm__("_F0.abi_mix");
extern float  abi_nine(float, float, float, float, float, float, float, float, float)
                                                   __asm__("_F0.abi_nine");
extern double abi_ten_mixed(double, float, double, float, double, float,
                            double, float, double, float)
                                                   __asm__("_F0.abi_ten_mixed");
extern float  abi_v2_sum(V2)                       __asm__("_F0.abi_v2_sum");
extern float  abi_v4_sum(V4)                       __asm__("_F0.abi_v4_sum");
extern double abi_d2_sum(D2)                       __asm__("_F0.abi_d2_sum");
extern double abi_if_sum(IntFloat)                 __asm__("_F0.abi_if_sum");
extern double abi_fi_sum(FloatInt)                 __asm__("_F0.abi_fi_sum");
extern float  abi_mixed_sum(Mixed)                 __asm__("_F0.abi_mixed_sum");
extern double abi_two_structs(V2, D2)              __asm__("_F0.abi_two_structs");
extern V2     abi_make_v2(float, float)            __asm__("_F0.abi_make_v2");
extern Mixed  abi_make_mixed(int, float)           __asm__("_F0.abi_make_mixed");
extern int    abi_caller_checks(void)              __asm__("_F0.abi_caller_checks");

/* --------------------------------------------------------- Firn caller */
/* The stubs in probe.fi are weakened by run.sh; these definitions win. */

float cimpl_add_f32(float a, float b) __asm__("_F0.cimpl_add_f32");
float cimpl_add_f32(float a, float b) { return a + b; }

double cimpl_add_f64(double a, double b) __asm__("_F0.cimpl_add_f64");
double cimpl_add_f64(double a, double b) { return a + b; }

double cimpl_mix(int a, float x, long b, double y, int c, float z, long d, double w)
    __asm__("_F0.cimpl_mix");
double cimpl_mix(int a, float x, long b, double y, int c, float z, long d, double w)
{ return a + (double)x + b + y + c + (double)z + d + w; }

float cimpl_nine(float a, float b, float c, float d, float e, float f,
                 float g, float h, float i) __asm__("_F0.cimpl_nine");
float cimpl_nine(float a, float b, float c, float d, float e, float f,
                 float g, float h, float i)
{ return a + b + c + d + e + f + g + h + i; }

double cimpl_ten_mixed(double a, float b, double c, float d, double e, float f,
                       double g, float h, double i, float j)
    __asm__("_F0.cimpl_ten_mixed");
double cimpl_ten_mixed(double a, float b, double c, float d, double e, float f,
                       double g, float h, double i, float j)
{ return a + b + c + d + e + f + g + h + i + j; }

float cimpl_v2_sum(V2 v) __asm__("_F0.cimpl_v2_sum");
float cimpl_v2_sum(V2 v) { return v.x + v.y; }

float cimpl_v4_sum(V4 v) __asm__("_F0.cimpl_v4_sum");
float cimpl_v4_sum(V4 v) { return v.x + v.y + v.z + v.w; }

double cimpl_d2_sum(D2 d) __asm__("_F0.cimpl_d2_sum");
double cimpl_d2_sum(D2 d) { return d.a + d.b; }

double cimpl_if_sum(IntFloat v) __asm__("_F0.cimpl_if_sum");
double cimpl_if_sum(IntFloat v) { return (double)v.a + v.x; }

double cimpl_fi_sum(FloatInt v) __asm__("_F0.cimpl_fi_sum");
double cimpl_fi_sum(FloatInt v) { return v.x + (double)v.a; }

float cimpl_mixed_sum(Mixed m) __asm__("_F0.cimpl_mixed_sum");
float cimpl_mixed_sum(Mixed m) { return (float)m.a + m.x; }

double cimpl_two_structs(V2 p, D2 q) __asm__("_F0.cimpl_two_structs");
double cimpl_two_structs(V2 p, D2 q) { return (double)p.x + (double)p.y + q.a + q.b; }

V2 cimpl_make_v2(float x, float y) __asm__("_F0.cimpl_make_v2");
V2 cimpl_make_v2(float x, float y) { V2 r; r.x = x; r.y = y; return r; }

Mixed cimpl_make_mixed(int a, float x) __asm__("_F0.cimpl_make_mixed");
Mixed cimpl_make_mixed(int a, float x) { Mixed r; r.a = a; r.x = x; return r; }

/* ------------------------------------------------------------- checking */

static int errors = 0;
static int checks = 0;

static void chk_d(const char *what, double got, double want)
{
    checks++;
    if (got != want) {
        printf("ERROR %-16s got %.17g, expected %.17g\n", what, got, want);
        errors++;
    }
}

static void chk_i(const char *what, long got, long want)
{
    checks++;
    if (got != want) {
        printf("ERROR %-16s got %ld, expected %ld\n", what, got, want);
        errors++;
    }
}

/* The second direction only runs where the Firn stubs really survived as
 * CALLS. With the optimizer switched on the inliner replaces a stub that
 * returns 0.0 by its result -- rightly so, it is Firn code and Firn knows
 * the body. Nothing of the calling convention would be measured then. In
 * the real case (a foreign library) that cannot happen; the body is not
 * there. That is why `run.sh` runs the caller direction on the build
 * without the optimizer, and the callee direction on both. */
int main(int argc, char **argv)
{
    int with_caller = (argc > 1 && strcmp(argv[1], "caller") == 0);

    /* --- direction 1: GCC calls Firn --- */
    chk_d("add_f32",      abi_add_f32(1.5f, 2.25f), 3.75);
    chk_d("add_f64",      abi_add_f64(1.5, 2.25), 3.75);
    chk_d("mix",          abi_mix(1, 2.0f, 3, 4.0, 5, 6.0f, 7, 8.0), 36.0);
    chk_d("nine",         abi_nine(1, 2, 3, 4, 5, 6, 7, 8, 9), 45.0);
    chk_d("ten_mixed",    abi_ten_mixed(1, 2, 3, 4, 5, 6, 7, 8, 9, 10), 55.0);

    V2 p = { 1.5f, 2.25f };
    V4 v = { 1.0f, 2.0f, 3.0f, 4.0f };
    D2 d = { 1.5, 2.5 };
    IntFloat w = { 10, 0.5 };
    FloatInt u = { 0.25, 4 };
    Mixed m = { 3, 0.5f };

    chk_d("v2_sum",       abi_v2_sum(p), 3.75);
    chk_d("v4_sum",       abi_v4_sum(v), 10.0);
    chk_d("d2_sum",       abi_d2_sum(d), 4.0);
    chk_d("if_sum",       abi_if_sum(w), 10.5);
    chk_d("fi_sum",       abi_fi_sum(u), 4.25);
    chk_d("mixed_sum",    abi_mixed_sum(m), 3.5);
    chk_d("two_structs",  abi_two_structs(p, d), 7.75);

    V2 r = abi_make_v2(1.5f, 2.25f);
    chk_d("make_v2.x",    r.x, 1.5);
    chk_d("make_v2.y",    r.y, 2.25);
    Mixed s = abi_make_mixed(7, 0.5f);
    chk_i("make_mixed.a", s.a, 7);
    chk_d("make_mixed.x", s.x, 0.5);

    /* --- direction 2: Firn calls GCC --- */
    if (with_caller) {
        int caller = abi_caller_checks();
        checks += 16;
        if (caller != 0) {
            printf("ERROR caller case %d failed (Firn -> GCC)\n", caller);
            errors++;
        }
    }

    printf("abi: %d checks, %d differing\n", checks, errors);
    return errors == 0 ? 0 : 1;
}
