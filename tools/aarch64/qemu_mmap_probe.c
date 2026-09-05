/* SPDX-License-Identifier: GPL-2.0-only */
/* tools/aarch64/qemu_mmap_probe.c -- does the RUNNER give a freed mapping
 * back at the same address?
 *
 * One case of the corpus (tests/1284_std_io_text_owner.fi) checks that the
 * comfort layer does not leak, and it checks it by the cheapest indicator
 * there is: two thousand rounds of allocate/free have to land on the same
 * address. That holds on Linux, because munmap really gives the address
 * range back. Under qemu-user it does NOT -- qemu manages the guest address
 * space itself and hands out fresh addresses.
 *
 * This program is the proof of that sentence, and it is a C program on
 * purpose: it contains no Firn, no firnc and no FIR. If it drifts, the
 * drift is the runner's, not the code generator's. If it does NOT drift,
 * tools/aarch64/run.sh counts the case as a real difference again --
 * the exception is only ever as good as this measurement. */
#include <stdio.h>
#include <sys/mman.h>

int main(void) {
    unsigned long first = 0, last = 0;
    for (int i = 0; i < 2000; i++) {
        void *p = mmap(0, 4096 * 4, PROT_READ | PROT_WRITE,
                       MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        if (p == MAP_FAILED) { printf("mmap failed\n"); return 2; }
        if (i == 0) first = (unsigned long) p;
        last = (unsigned long) p;
        munmap(p, 4096 * 4);
    }
    long drift = (long) (last - first);
    if (drift < 0) drift = -drift;
    printf("mmap-address-reuse: %s (drift %ld bytes over 2000 rounds)\n",
           drift < 65536 ? "yes" : "no", drift);
    return drift < 65536 ? 0 : 1;   /* 0 = addresses are reused */
}
