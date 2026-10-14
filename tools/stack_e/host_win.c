/* SPDX-License-Identifier: MPL-2.0
 * tools/stack_e/host_win.c -- Windows host (mingw, run under Wine): LoadLibrary a Firn DLL built with
 * `firnc --shared --target=x86_64-windows` and call its exports. argv[1] = DLL, argv[2] = "win" | "rt". */
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
static int bad = 0;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL " __VA_ARGS__); printf("\n"); bad++; } } while (0)
typedef struct { uint32_t a, b; } Pair;
int main(int argc, char **argv) {
    if (argc < 3) return 2;
    HMODULE h = LoadLibraryA(argv[1]);
    if (!h) { printf("FAIL LoadLibrary %lu\n", GetLastError()); return 1; }
    if (!strcmp(argv[2], "win")) {
        uint64_t (*abi)(void) = (void *)GetProcAddress(h, "wd_abi");
        int32_t (*add)(int32_t, int32_t) = (void *)GetProcAddress(h, "wd_add_i32");
        int64_t (*mix)(int32_t, int64_t, uint8_t, uint32_t, uint64_t, int16_t) = (void *)GetProcAddress(h, "wd_mix6");
        void (*swap)(Pair *) = (void *)GetProcAddress(h, "wd_pair_swap");
        uint64_t (*sum)(const uint8_t *, size_t) = (void *)GetProcAddress(h, "wd_sum_u8");
        uint64_t (*inc)(void) = (void *)GetProcAddress(h, "wd_count_inc");
        uint8_t (*bnot)(uint8_t) = (void *)GetProcAddress(h, "wd_bool_not");
        uint64_t (*tl)(uint64_t) = (void *)GetProcAddress(h, "wd_gc_chains");
        uint64_t (*init)(void) = (void *)GetProcAddress(h, "wd_init");
        CHECK(abi && add && mix && swap && sum && inc && bnot && tl && init, "GetProcAddress");
        if (bad) return 1;
        CHECK(init() == 1, "gc_init");
        CHECK(abi() == 5, "abi");
        CHECK(add(-7, 12) == 5, "add");
        CHECK(mix(1, 2, 3, 4, 5, -6) == 1 + 4 + 9 + 16 + 25 - 36, "mix6 %lld", (long long)mix(1, 2, 3, 4, 5, -6));
        Pair p = { 1, 2 }; swap(&p);
        CHECK(p.a == 2 && p.b == 1, "pair");
        uint8_t buf[100]; for (int i = 0; i < 100; i++) buf[i] = (uint8_t)i;
        CHECK(sum(buf, 100) == 4950, "sum");
        CHECK(inc() == 1 && inc() == 2 && inc() == 3, "static counter");
        CHECK(bnot(0) == 1 && bnot(1) == 0, "bool");
        CHECK(tl(16) == 16ull*820, "gc_chains %llu", (unsigned long long)tl(16));
        for (int i = 0; i < 40; i++) tl(2000); /* GC churn */
        CHECK(tl(8) == 8ull*820, "gc_chains after churn");
    } else {
        uint64_t (*heap)(size_t) = (void *)GetProcAddress(h, "se_heap_probe");
        uint64_t (*jobs)(void *, uint64_t, uint64_t) = (void *)GetProcAddress(h, "se_jobs_probe");
        uint64_t (*gc)(uint64_t) = (void *)GetProcAddress(h, "se_gc_probe");
        CHECK(heap && jobs && gc, "GetProcAddress rt");
        if (bad) return 1;
        CHECK(heap(1000) == 499500, "heap_alloc");
        static uint32_t out[8192];
        CHECK(jobs(out, 4096, 0) == 4096 && out[100] == 300 && out[4095] == 12285, "jobs 0 workers");
        memset(out, 0, sizeof out);
        CHECK(jobs(out, 4096, 3) == 4096 && out[100] == 300 && out[4095] == 12285, "jobs 3 workers");
        CHECK(gc(3) == 1600, "gc 4 threads %llu", (unsigned long long)gc(3));
    }
    if (!bad) printf("host_win %s: all ok\n", argv[2]);
    return bad ? 1 : 0;
}
