// tools/stack_e/host_attach_win.c -- Windows twin of host_attach.c (mingw, run under Wine): several CreateThread
// threads of the HOST call into the Firn DLL (r506)
// (jobs + GC), each registers itself with se_attach and leaves with se_detach. Two waves (entries are
// recycled), one thread idles in host code while the others collect, host threads also run libc work.
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define usleep(us) Sleep((us) / 1000)

static uint64_t (*attach)(void), (*detach)(void), (*threads)(void), (*gcp)(uint64_t), (*gcr)(uint64_t, uint64_t);
static uint64_t (*jobs)(void *, uint64_t, uint64_t);
static volatile int fails = 0;
static __thread int tls_val;

static void fail(const char *m, long id, unsigned long v) { printf("FAIL %s (thread %ld, %lu)\n", m, id, v); fails++; }

static DWORD WINAPI worker(LPVOID arg) {
    long id = (long)(intptr_t)arg;
    tls_val = (int)id * 5;
    if (attach() != 1) { fail("attach", id, 0); return 0; }
    if (attach() != 1) { fail("attach twice", id, 0); }            // idempotent
    uint32_t *buf = calloc(8192, 4);
    for (int rep = 0; rep < 3; rep++) {
        if (id == 3) usleep(150000);                                // idles in host code (BLOCKED) while others collect
        for (uint64_t w = 0; w <= 3; w += 3) {
            memset(buf, 0, 8192 * 4);
            uint64_t r = jobs(buf, 4096, w);
            if (r != 4096 || buf[100] != 300 || buf[4095] != 12285) fail("jobs", id, r);
        }
        uint64_t r = gcp((uint64_t)(id % 3));
        if (r != 400 * ((id % 3) + 1)) fail("gc workers", id, r);
        r = gcr(id * 1000 + 7, 300);
        if (r != 300) fail("gc rounds", id, r);
        char *p = malloc(64); snprintf(p, 64, "t%ld", id); if (p[0] != 't') fail("malloc", id, 0); free(p);
        if (tls_val != (int)id * 5) fail("host tls", id, 0);
    }
    free(buf);
    if (detach() != 1) fail("detach", id, 0);
    if (detach() != 0) fail("detach twice", id, 0);                 // not attached any more
    return 0;
}

int main(int argc, char **argv) {
    HMODULE h = LoadLibraryA(argv[1]);
    if (!h) { printf("LoadLibrary failed: %lu\n", GetLastError()); return 2; }
    attach = (void *)GetProcAddress(h, "se_attach"); detach = (void *)GetProcAddress(h, "se_detach");
    threads = (void *)GetProcAddress(h, "se_threads");
    gcp = (void *)GetProcAddress(h, "se_gc_probe_h"); gcr = (void *)GetProcAddress(h, "se_gc_rounds_h");
    jobs = (void *)GetProcAddress(h, "se_jobs_probe_h");
    if (!attach || !detach || !threads || !gcp || !gcr || !jobs) { printf("missing symbol\n"); return 3; }
    // The main thread is the first to use the runtime: it becomes the Firn main thread and stays alive.
    if (attach() != 1) { printf("FAIL main attach\n"); return 1; }
    if (detach() != 0) { printf("FAIL main must not detach\n"); fails++; }
    for (int wave = 0; wave < 2; wave++) {
        HANDLE t[4];
        for (long i = 0; i < 4; i++) t[i] = CreateThread(0, 0, worker, (LPVOID)(intptr_t)i, 0, 0);
        uint64_t r = gcr(99, 200);                                  // the main thread works too
        if (r != 200) fail("main gc rounds", 99, r);
        for (int i = 0; i < 4; i++) { WaitForSingleObject(t[i], INFINITE); CloseHandle(t[i]); }
        uint64_t n = threads();
        if (n != 1) fail("thread count after wave", wave, n);
    }
    printf(fails ? "FAIL host_attach (%d)\n" : "ok  host_attach\n", fails);
    return fails != 0;
}
