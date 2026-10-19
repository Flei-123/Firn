// JSON-like text generated in memory (350k records, ~28 MB), then scanned 4 times by
// an own tokenizer: counts objects, arrays, numbers, strings (keys included) and
// sums the numbers (signed, wrapping). Escapes inside strings are skipped.
// Firn counterpart: bench/firn_vs_c/firn/jsonscan.fi
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

static inline u64 lcg(u64 s) { return s * 6364136223846793005ULL + 1442695040888963407ULL; }
#pragma GCC optimize("no-tree-loop-distribute-patterns")

static const char F0[] = "{\"id\":";
static const char F1[] = ",\"name\":\"it\\\"em";
static const char F2[] = "\\\"\",\"ok\":";
static const char F3[] = "true";
static const char F4[] = "false";
static const char F5[] = ",\"v\":[";
static const char F6[] = "],\"o\":{\"p\":";
static const char F7[] = "}},";

static u8 *G_BUF;
static size_t G_AT;

static inline void push_byte(u8 b) { G_BUF[G_AT++] = b; }

static void push_lit(const char *src, size_t n) {
    for (size_t j = 0; j < n; j++) G_BUF[G_AT + j] = (u8)src[j];
    G_AT += n;
}

static void push_dec(u64 v) {
    u8 tmp[24];
    size_t n = 0;
    u64 x = v;
    if (x == 0) { tmp[0] = 48; n = 1; }
    while (x > 0) { tmp[n++] = (u8)(x % 10) + 48; x /= 10; }
    while (n > 0) { n--; push_byte(tmp[n]); }
}

static inline int is_digit(u8 c) { return c >= 48 && c <= 57; }

static u64 scan(const u8 *p, size_t len) {
    size_t i = 0;
    u64 objects = 0, arrays = 0, numbers = 0, strings = 0, sum = 0;
    while (i < len) {
        u8 c = p[i];
        if (c == 123) {
            objects++;
            i++;
        } else if (c == 91) {
            arrays++;
            i++;
        } else if (c == 34) {
            strings++;
            i++;
            while (i < len) {
                u8 d = p[i];
                if (d == 92) {
                    i += 2;
                } else if (d == 34) {
                    i++;
                    break;
                } else {
                    i++;
                }
            }
        } else if (is_digit(c) || c == 45) {
            int neg = 0;
            if (c == 45) { neg = 1; i++; }
            u64 v = 0;
            while (i < len) {
                u8 d = p[i];
                if (is_digit(d)) {
                    v = v * 10 + (u64)(d - 48);
                    i++;
                } else {
                    break;
                }
            }
            numbers++;
            if (neg) sum -= v; else sum += v;
        } else {
            i++;
        }
    }
    u64 r = objects;
    r = r * 1000003ULL + arrays;
    r = r * 1000003ULL + numbers;
    r = r * 1000003ULL + strings;
    r = r * 1000003ULL + sum;
    return r;
}

int main(void) {
    u64 count = 350000;
    G_BUF = (u8 *)calloc((size_t)count * 128, 1);
    G_AT = 0;
    u64 s = 21;
    for (u64 i = 0; i < count; i++) {
        s = lcg(s);
        push_lit(F0, 6);
        push_dec(i);
        push_lit(F1, 15);
        push_dec((s >> 40) % 100000);
        push_lit(F2, 9);
        if (((s >> 20) & 1) == 1) push_lit(F3, 4);
        else push_lit(F4, 5);
        push_lit(F5, 6);
        push_dec((s >> 8) % 1000);
        push_byte(44);
        push_byte(45);
        push_dec((s >> 30) % 1000);
        push_byte(44);
        push_dec((s >> 50) % 100000);
        push_lit(F6, 11);
        push_dec((s >> 12) % 100);
        push_lit(F7, 3);
    }
    u64 total = 0;
    for (u32 pass = 0; pass < 4; pass++) total = total * 1000003ULL + scan(G_BUF, G_AT);
    printf("%llu\n", (unsigned long long)total);
    return 0;
}
