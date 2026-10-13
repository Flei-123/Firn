/* tools/aarch64/qemu_rss_probe.c -- does the RUNNER's resident memory grow
 * while a program only maps and unmaps memory?
 *
 * tests/2189_compress_soak.fi measures a leak through /proc/self/statm. Under
 * qemu-user that file describes the HOST process of qemu, and qemu hands out a
 * fresh guest address for every mapping (see qemu_mmap_probe.c); its own page
 * bookkeeping for those fresh addresses is never given back. The result: the
 * resident size climbs with every map/unmap round, although the guest frees
 * everything.
 *
 * This is a C program on purpose: no Firn, no firnc, no FIR. 1500 rounds of
 * mmap (8 MiB, one page touched) + munmap. Natively the resident size stays
 * flat. If it grows here, the growth belongs to the runner. If it does NOT
 * grow, tools/aarch64/run.sh counts the case as a real difference again. */
#include <stdio.h>
#include <sys/mman.h>

static long rss_pages(void) {
    long a = 0, b = -1;
    FILE *f = fopen("/proc/self/statm", "r");
    if (f) { if (fscanf(f, "%ld %ld", &a, &b) != 2) b = -1; fclose(f); }
    return b;
}

int main(void) {
    const size_t n = 8u * 1024 * 1024;
    long start = rss_pages();
    for (int i = 0; i < 1500; i++) {
        volatile char *p = mmap(0, n, PROT_READ | PROT_WRITE,
                                MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        if (p == MAP_FAILED) { printf("mmap failed\n"); return 2; }
        p[0] = 1;
        munmap((void *) p, n);
    }
    long grown = rss_pages() - start;
    printf("mmap-rss-growth: %s (%ld pages over 1500 map/unmap rounds)\n",
           grown > 1024 ? "yes" : "no", grown);
    return grown > 1024 ? 1 : 0;   /* 0 = resident size stays flat */
}
