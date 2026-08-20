/* demos/kernel/kernel.ld -- round 59: the layout of the small kernel.
 *
 * Binds `boot.s` (multiboot header, long mode), `isr.s` (interrupt entry
 * points) and the freestanding object file out of `kmain.fi` into an image
 * that `qemu-system-x86_64 -kernel` loads. No libc, no start files
 * (crt*.o), no dynamic linker.
 *
 * Three things this script decides and the kernel relies on:
 *
 *   1. the multiboot header lies in the first 8 KiB -- therefore first,
 *      immediately followed by the 32-bit prologue,
 *   2. `.utext` gets a 4 KiB page OF ITS OWN. The kernel sets the
 *      user bit for exactly that page; without a page of its own, ring 3
 *      would be able to read kernel code,
 *   3. `kernel_end` marks the first byte behind the image. The frame
 *      allocator reads it and never hands out anything below it.
 *
 * `KERNEL_MAIN`, `KERNEL_TRAP` and `KERNEL_SYSCALL` are set by `ld --defsym`
 * when linking: firnc0 puts `_F0.` in front of every symbol, firnc1
 * `_F1.` (docs/SELF_HOSTING.md).
 */
ENTRY(_boot)

SECTIONS
{
    . = 1M;

    .text : ALIGN(4K)
    {
        *(.multiboot)
        *(.text.boot)
        *(.text .text.*)
    }

    .utext : ALIGN(4K)
    {
        *(.utext)
        . = ALIGN(4K);
    }

    .rodata : ALIGN(4K) { *(.rodata .rodata.*) }
    .data   : ALIGN(4K) { *(.data .data.*) }
    .bss    : ALIGN(4K) { *(COMMON) *(.bss .bss.*) }

    kernel_end = .;

    /DISCARD/ : { *(.note.GNU-stack) *(.comment) *(.eh_frame) }
}
