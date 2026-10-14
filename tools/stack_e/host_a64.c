/* SPDX-License-Identifier: MPL-2.0
 * tools/stack_e/host_a64.c -- aarch64 Linux host (run under qemu-aarch64): dlopen a Firn .so built with
 * `firnc --shared --target=aarch64-linux` (lib_win.fi probe library) and call its exports.
 * The host uses glibc TLS / errno / stack protector between the calls: the .so must leave tpidr_el0 alone. */
#include <dlfcn.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
static int bad = 0;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL " __VA_ARGS__); printf("\n"); bad++; } } while (0)
typedef struct { uint32_t a, b; } Pair;
static __thread int tls_probe = 1234;
int main(int argc, char **argv) {
    if (argc < 2) return 2;
    void *h = dlopen(argv[1], RTLD_NOW);
    if (!h) { printf("FAIL dlopen %s\n", dlerror()); return 1; }
    uint64_t (*abi)(void) = dlsym(h, "wd_abi");
    int32_t (*add)(int32_t, int32_t) = dlsym(h, "wd_add_i32");
    int64_t (*mix)(int32_t, int64_t, uint8_t, uint32_t, uint64_t, int16_t) = dlsym(h, "wd_mix6");
    void (*swap)(Pair *) = dlsym(h, "wd_pair_swap");
    uint64_t (*sum)(const uint8_t *, size_t) = dlsym(h, "wd_sum_u8");
    uint64_t (*inc)(void) = dlsym(h, "wd_count_inc");
    uint8_t (*bnot)(uint8_t) = dlsym(h, "wd_bool_not");
    uint64_t (*tl)(uint64_t) = dlsym(h, "wd_gc_chains");
    uint64_t (*init)(void) = dlsym(h, "wd_init");
    CHECK(abi && add && mix && swap && sum && inc && bnot && tl && init, "dlsym");
    if (bad) return 1;
    errno = 77;
    CHECK(init() == 1, "gc_init");
    CHECK(abi() == 5, "abi");
    CHECK(add(-7, 12) == 5, "add");
    CHECK(mix(1, 2, 3, 4, 5, -6) == 1 + 4 + 9 + 16 + 25 - 36, "mix6");
    Pair p = { 1, 2 }; swap(&p);
    CHECK(p.a == 2 && p.b == 1, "pair");
    uint8_t buf[100]; for (int i = 0; i < 100; i++) buf[i] = (uint8_t)i;
    CHECK(sum(buf, 100) == 4950, "sum");
    CHECK(inc() == 1 && inc() == 2 && inc() == 3, "static counter");
    CHECK(bnot(0) == 1 && bnot(1) == 0, "bool");
    CHECK(tl(16) == 16ull * 820, "gc_chains");
    for (int i = 0; i < 8; i++) tl(2000); /* GC churn */
    CHECK(tl(8) == 8ull * 820, "gc_chains after churn");
    CHECK(tls_probe == 1234 && errno == 77, "host TLS / errno intact (tpidr_el0 untouched)");
    if (!bad) printf("host_a64: all ok\n");
    return bad ? 1 : 0;
}
