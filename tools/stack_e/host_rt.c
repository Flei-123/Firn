// tools/stack_e/host_rt.c -- C host for the runtime-in-a-.so probe (G1): Firn threads + GC inside a .so
// while the host runs threads of its own (malloc, stdio, stack protector, errno, qsort, TLS).
#include <dlfcn.h>
#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static volatile int stop_flag = 0, host_bad = 0;
static __thread int tls_val = 0;

static int cmp(const void *a, const void *b) { return *(const int *)a - *(const int *)b; }

// Everything here touches the glibc TCB (canary on the stack, errno, TLS, malloc arena).
static void *host_thread(void *arg) {
    long id = (long)arg;
    tls_val = (int)id * 7;
    unsigned long iter = 0;
    while (!stop_flag) {
        int v[64];
        for (int i = 0; i < 64; i++) v[i] = (int)((i * 2654435761u + id) & 1023);
        qsort(v, 64, sizeof v[0], cmp);
        for (int i = 1; i < 64; i++) if (v[i - 1] > v[i]) host_bad = 1;
        char *p = malloc(100 + (iter & 255));
        snprintf(p, 100, "t%ld i%lu", id, iter);
        if (strncmp(p, "t", 1) != 0) host_bad = 1;
        free(p);
        errno = (int)id;
        if (errno != (int)id) host_bad = 1;
        if (tls_val != (int)id * 7) host_bad = 1;
        iter++;
    }
    return (void *)iter;
}

int main(int argc, char **argv) {
    void *h = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!h) { printf("dlopen failed: %s\n", dlerror()); return 2; }
    uint64_t (*jobs)(void *, uint64_t, uint64_t) = dlsym(h, "se_jobs_probe");
    uint64_t (*gcp)(uint64_t) = dlsym(h, "se_gc_probe");
    if (!jobs || !gcp) { printf("missing symbol\n"); return 3; }
    int fails = 0;
    pthread_t t[4];
    for (long i = 0; i < 4; i++) pthread_create(&t[i], 0, host_thread, (void *)i);
    uint32_t *buf = calloc(8192, 4);
    for (int rep = 0; rep < 3; rep++) {
        for (uint64_t w = 0; w <= 3; w += 3) {
            memset(buf, 0, 8192 * 4);
            uint64_t r = jobs(buf, 4096, w);
            if (r != 4096 || buf[100] != 300 || buf[4095] != 12285) { printf("FAIL jobs w=%lu r=%lu\n", w, r); fails++; }
        }
    }
    for (uint64_t w = 0; w <= 3; w++) {
        uint64_t r = gcp(w);
        if (r != 400 * (w + 1)) { printf("FAIL gc w=%lu r=%lu\n", w, r); fails++; }
    }
    stop_flag = 1;
    unsigned long total = 0;
    for (int i = 0; i < 4; i++) { void *x; pthread_join(t[i], &x); total += (unsigned long)x; }
    if (host_bad) { printf("FAIL host threads saw corruption\n"); fails++; }
    printf("host threads ran %lu iterations\n", total);
    free(buf);
    printf(fails ? "FAIL host_rt (%d)\n" : "ok  host_rt\n", fails);
    return fails != 0;
}
