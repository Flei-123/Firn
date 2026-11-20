#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include "ffi_features.h"

long fi_get(struct bits *p, int which);
void fi_set(struct bits *p, int which, long x);
long fi_varargs(void);
long fi_consts_int(long, long, long, long, long, long);
long fi_consts_text(long, long, long);

int vsum(int n, ...) {
    va_list ap;
    int s = 0;
    va_start(ap, n);
    for (int i = 0; i < n; i++) s += va_arg(ap, int);
    va_end(ap);
    return s;
}

double dmix(const char *tag, int n, ...) {
    va_list ap;
    double s = tag[0];
    va_start(ap, n);
    int a = va_arg(ap, int);
    int b = va_arg(ap, int);
    double d = va_arg(ap, double);
    va_end(ap);
    return s * 0 + a + b + d + n * 0;
}

static unsigned long long state = 88172645463325252ULL;
static unsigned long next(void) {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    return (unsigned long)(state >> 11);
}

static long c_get(struct bits *p, int which) {
    switch (which) {
    case 0: return p->a;
    case 1: return p->b;
    case 2: return p->c;
    case 3: return p->d;
    case 4: return p->f;
    default: return p->g;
    }
}

static void c_set(struct bits *p, int which, long x) {
    switch (which) {
    case 0: p->a = x; break;
    case 1: p->b = x; break;
    case 2: p->c = x; break;
    case 3: p->d = x; break;
    case 4: p->f = x; break;
    default: p->g = x; break;
    }
}

int main(void) {
    int bad = 0;
    for (int iter = 0; iter < 20000; iter++) {
        struct bits x, y;
        unsigned char *raw = (unsigned char *)&x;
        for (unsigned i = 0; i < sizeof x; i++) raw[i] = next();
        y = x;
        for (int which = 0; which < 6; which++) {
            if (fi_get(&x, which) != c_get(&x, which)) bad++;
        }
        int w = next() % 6;
        long v = (w == 1 || w == 4) ? (long)(int)(unsigned)next() : (long)(unsigned)next();
        c_set(&y, w, v);
        fi_set(&x, w, v);
        if (memcmp(&x, &y, sizeof x) != 0) bad++;
    }
    if (bad) { printf("bitfield mismatches: %d\n", bad); return 1; }
    long exp = (long)(vsum(3, 10, 20, 30) + vsum(1, 7)) * 1000 + (long)(dmix("x", 2, 3, 4, 4.5) * 10.0);
    long va = fi_varargs();
    if (va != exp) { printf("varargs %ld vs %ld\n", va, exp); return 1; }
    if (fi_consts_int(HDR_SIZE, PTR_SIZE, ULONG_SIZE, SHORT_SIZE, HDR_BYTES, BUF_BYTES) != 0) { printf("int consts differ\n"); return 1; }
    if (fi_consts_text((long)(PI_F * 1000000.0), (long)(NEG_E * 1000000.0), (long)(HALF * 1000000.0)) != 0) { printf("text consts differ\n"); return 1; }
    return 0;
}
