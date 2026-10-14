// tools/stack_e/host_noattach.c -- r506: host pthreads that NEVER call se_attach. The compiler-inserted export guard
// registers them on their first export call (and puts them back into host mode on exit), so jobs, GC and the
// collector's stop-the-world work although one thread idles in host code. Threads end with and without se_detach;
// wave 2 reuses thread ids (stale entries are recycled), wave 3 is the clean-up.
#include <dlfcn.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static uint64_t (*attach)(void), (*detach)(void), (*threads)(void), (*gcp)(uint64_t), (*gcph)(uint64_t);
static uint64_t (*jobs)(void *, uint64_t, uint64_t);
static volatile int fails = 0;
static int final_wave = 0;

static void fail(const char *m, long id, unsigned long v) { printf("FAIL %s (thread %ld, %lu)\n", m, id, v); fails++; }

static void *worker(void *arg) {
    long id = (long)arg;
    uint32_t *buf = calloc(8192, 4);
    for (int rep = 0; rep < 3; rep++) {
        if (id == 3) usleep(150000);                                // idles in host code while the others collect
        for (uint64_t w = 0; w <= 3; w += 3) {
            memset(buf, 0, 8192 * 4);
            uint64_t r = jobs(buf, 4096, w);
            if (r != 4096 || buf[100] != 300 || buf[4095] != 12285) fail("jobs", id, r);
        }
        uint64_t r = gcp((uint64_t)(id % 3));
        if (r != 400 * ((id % 3) + 1)) fail("gc workers", id, r);
    }
    free(buf);
    if (final_wave || id < 2) {
        if (detach() != 1) fail("detach", id, 0);                   // the guard attached it; detach works
    }
    return 0;
}

int main(int argc, char **argv) {
    void *h = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!h) { printf("dlopen failed: %s\n", dlerror()); return 2; }
    attach = dlsym(h, "se_attach"); detach = dlsym(h, "se_detach"); threads = dlsym(h, "se_threads");
    gcp = dlsym(h, "se_gc_probe"); gcph = dlsym(h, "se_gc_probe_h"); jobs = dlsym(h, "se_jobs_probe");
    if (!attach || !detach || !threads || !gcp || !gcph || !jobs) { printf("missing symbol\n"); return 3; }
    if (attach() != 1) { printf("FAIL main attach\n"); return 1; }  // first runtime user = Firn main thread
    for (int wave = 0; wave < 3; wave++) {
        final_wave = wave == 2;
        pthread_t t[4];
        for (long i = 0; i < 4; i++) pthread_create(&t[i], 0, worker, (void *)i);
        uint64_t r = gcph(1);                                       // the main thread (attached, bracketed) collects too
        if (r != 800) fail("main gc", 99, r);
        for (int i = 0; i < 4; i++) pthread_join(t[i], 0);
    }
    printf(fails ? "FAIL host_noattach (%d)\n" : "ok  host_noattach\n", fails);
    return fails != 0;
}
