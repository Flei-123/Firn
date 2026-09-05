/* SPDX-License-Identifier: GPL-2.0-only */
/* tools/aarch64/abi_host.c -- the C side of the AAPCS64 proof. Compiled by
 * aarch64-linux-gnu-gcc, which knows nothing about Firn and follows the
 * written standard. Exit code = the number of disagreements. */
#include <stdio.h>

long firn_ints(long, long, long, long, long, long, long, long, long, long);
double firn_floats(double, double, double, double, double, double, double, double, double);
long firn_calls_c(void);
long firn_calls_c_floats(void);

long c_sum10(long a, long b, long c, long d, long e, long f, long g, long h, long i, long j) {
    return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f + 7 * g + 8 * h + 9 * i + 10 * j;
}
double c_fsum9(double a, double b, double c, double d, double e, double f, double g, double h, double i) {
    return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f + 7 * g + 8 * h + 9 * i;
}

int main(void) {
    int bad = 0;
    long r1 = firn_ints(1, 2, 3, 4, 5, 6, 7, 8, 9, 10);   /* 1+4+9+...+100 */
    if (r1 != 385) { printf("  FAIL C calls Firn, ten integer words: %ld, expected 385\n", r1); bad++; }
    else printf("  ok   C calls Firn, ten integer words (two on the stack)\n");

    double r2 = firn_floats(1, 2, 3, 4, 5, 6, 7, 8, 9);   /* 1+4+9+...+81 */
    if (r2 != 285.0) { printf("  FAIL C calls Firn, nine floating point words: %f, expected 285\n", r2); bad++; }
    else printf("  ok   C calls Firn, nine floating point words (one on the stack)\n");

    long r3 = firn_calls_c();
    if (r3 != 385) { printf("  FAIL Firn calls C, ten integer words: %ld, expected 385\n", r3); bad++; }
    else printf("  ok   Firn calls C, ten integer words\n");

    long r4 = firn_calls_c_floats();
    if (r4 != 28500) { printf("  FAIL Firn calls C, nine floating point words: %ld, expected 28500\n", r4); bad++; }
    else printf("  ok   Firn calls C, nine floating point words\n");

    if (bad == 0) printf("  aapcs64: 4/4 agree with aarch64-linux-gnu-gcc\n");
    return bad;
}
