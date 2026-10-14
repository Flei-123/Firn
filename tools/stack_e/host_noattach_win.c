// tools/stack_e/host_noattach_win.c -- Windows twin of host_noattach.c (mingw, run under Wine): CreateThread threads of the
// host that NEVER call se_attach. The export guard (TLS slot test) registers them on their first export call and puts them
// back into host mode on exit; threads end with and without se_detach, wave 2 reuses thread ids.
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define usleep(us) Sleep((us) / 1000)

static uint64_t (*attach)(void), (*detach)(void), (*gcp)(uint64_t), (*gcph)(uint64_t);
static uint64_t (*jobs)(void *, uint64_t, uint64_t);
static volatile int fails = 0;
static int final_wave = 0;

static void fail(const char *m, long id, unsigned long v) { printf("FAIL %s (thread %ld, %lu)\n", m, id, v); fails++; }

static DWORD WINAPI worker(LPVOID arg) {
    long id = (long)(intptr_t)arg;
    uint32_t *buf = calloc(8192, 4);
    for (int rep = 0; rep < 3; rep++) {
        if (id == 3) usleep(150000);
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
        if (detach() != 1) fail("detach", id, 0);
    }
    return 0;
}

int main(int argc, char **argv) {
    HMODULE h = LoadLibraryA(argv[1]);
    if (!h) { printf("LoadLibrary failed: %lu\n", GetLastError()); return 2; }
    attach = (void *)GetProcAddress(h, "se_attach"); detach = (void *)GetProcAddress(h, "se_detach");
    gcp = (void *)GetProcAddress(h, "se_gc_probe"); gcph = (void *)GetProcAddress(h, "se_gc_probe_h");
    jobs = (void *)GetProcAddress(h, "se_jobs_probe");
    if (!attach || !detach || !gcp || !gcph || !jobs) { printf("missing symbol\n"); return 3; }
    if (attach() != 1) { printf("FAIL main attach\n"); return 1; }
    for (int wave = 0; wave < 3; wave++) {
        final_wave = wave == 2;
        HANDLE t[4];
        for (long i = 0; i < 4; i++) t[i] = CreateThread(0, 0, worker, (LPVOID)(intptr_t)i, 0, 0);
        uint64_t r = gcph(1);
        if (r != 800) fail("main gc", 99, r);
        for (int i = 0; i < 4; i++) { WaitForSingleObject(t[i], INFINITE); CloseHandle(t[i]); }
    }
    printf(fails ? "FAIL host_noattach (%d)\n" : "ok  host_noattach\n", fails);
    return fails != 0;
}
