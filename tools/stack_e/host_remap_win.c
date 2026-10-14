// tools/stack_e/host_remap_win.c -- r576 (mingw, run under Wine): the collector's "is this stack still mapped" probe
// (win_seam.rs, madvise -> VirtualQuery) must also see that the stack of a host thread that ended WITHOUT se_detach now
// holds something else. The old probe looked at the first and the last page and took "committed" for "mapped": a stack
// range that was meanwhile re-mapped as no-access / guard pages passed, and the next GC scan faulted.
// Thread A and B call an export (the export guard registers them) and end; the test waits until the system has freed
// their stacks (Wine: frees them itself), maps PAGE_NOACCESS (A) / PAGE_READWRITE|PAGE_GUARD (B) memory over the top of the old stack, and
// forces collections. A re-mapping with ordinary read-write pages is scanned harmlessly (conservative); it is run, too.
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static uint64_t (*attach)(void), (*gcph)(uint64_t);
static uint64_t (*jobs)(void *, uint64_t, uint64_t);
static volatile int fails = 0;

typedef struct { uintptr_t base, limit; } Stk;
static Stk stk[3];
static DWORD protect_of[3] = { PAGE_NOACCESS, PAGE_READWRITE | PAGE_GUARD, PAGE_READWRITE };

static DWORD WINAPI worker(LPVOID arg) {
    int id = (int)(intptr_t)arg;
    static uint32_t buf[8192];
    memset(buf, 0, sizeof buf);
    for (int w = 0; w < 3; w++) jobs(buf, 4096, 0);   // the guard registers this thread on its first export call
    NT_TIB *tib = (NT_TIB *)NtCurrentTeb();
    stk[id].base = (uintptr_t)tib->StackBase;
    stk[id].limit = (uintptr_t)tib->StackLimit;
    return 0;   // no se_detach: the entry stays behind, the system frees the stack
}

int main(int argc, char **argv) {
    HMODULE h = LoadLibraryA(argv[1]);
    if (!h) { printf("LoadLibrary failed: %lu\n", GetLastError()); return 2; }
    attach = (void *)GetProcAddress(h, "se_attach");
    gcph = (void *)GetProcAddress(h, "se_gc_probe_h");
    jobs = (void *)GetProcAddress(h, "se_jobs_probe");
    if (!attach || !gcph || !jobs) { printf("missing symbol\n"); return 3; }
    if (attach() != 1) { printf("FAIL main attach\n"); return 1; }
    for (int id = 0; id < 3; id++) {
        HANDLE t = CreateThread(0, 0, worker, (LPVOID)(intptr_t)id, 0, 0);
        WaitForSingleObject(t, INFINITE);
        CloseHandle(t);
        // Windows frees the stack of an ended thread by itself; Wine keeps it, so free it here (a failure on Windows is harmless:
        // there it is gone already), then wait until it is gone either way
        MEMORY_BASIC_INFORMATION mbi;
        if (VirtualQuery((void *)(stk[id].base - 4096), &mbi, sizeof mbi) && mbi.State == MEM_COMMIT) VirtualFree(mbi.AllocationBase, 0, MEM_RELEASE);
        int gone = 0;
        for (int spin = 0; spin < 500 && !gone; spin++) {
            if (!VirtualQuery((void *)(stk[id].base - 4096), &mbi, sizeof mbi) || mbi.State != MEM_COMMIT) gone = 1; else Sleep(10);
        }
        if (!gone) { printf("FAIL thread %d: stack was never freed\n", id); fails++; continue; }
        // re-map the top of the old stack: 4 allocation-granularity blocks below its end
        uintptr_t top = (stk[id].base + 0xFFFF) & ~(uintptr_t)0xFFFF;
        int mapped = 0;
        for (int k = 1; k <= 4; k++) {
            void *p = VirtualAlloc((void *)(top - 0x10000 * k), 0x10000, MEM_RESERVE | MEM_COMMIT, protect_of[id]);
            if (p) mapped++;
        }
        if (!mapped) { printf("FAIL thread %d: nothing could be mapped over the old stack\n", id); fails++; continue; }
        fflush(stdout);
        for (int r = 0; r < 3; r++) {
            uint64_t v = gcph(1);   // a full collection scans the table, including the stale entry
            if (v != 800) { printf("FAIL gc after re-map %d: %llu\n", id, (unsigned long long)v); fails++; break; }
        }
    }
    printf(fails ? "FAIL host_remap (%d)\n" : "ok  host_remap\n", fails);
    return fails != 0;
}
