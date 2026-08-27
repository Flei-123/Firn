/* demos/freestanding/a64/linker.ld -- ROUND ARM-FREESTANDING.
 *
 * Binds `start.s` (entry, vector table, osum_panic) and the freestanding
 * object file of `core.fi` into an image that `qemu-system-aarch64 -M virt
 * -kernel` loads. NO libc, no start files (crt*.o), no dynamic linker.
 *
 * 0x4008_0000 is where an image for the `virt` machine goes: RAM starts at
 * 0x4000_0000 and the first half megabyte is the place every loader for
 * this board (and the Linux kernel's own arm64 boot protocol) expects the
 * image at.
 *
 * `KERN_START` and `KERN_TRAP` are set at the link step (`ld --defsym`):
 * firnc0 prefixes every symbol with `_F0.`, firnc1 with `_F1.`
 * (docs/SELF_HOSTING.md).
 */
ENTRY(_boot)

SECTIONS
{
    . = 0x40080000;

    .text : ALIGN(4K)
    {
        *(.text.boot)
        *(.text.vec)
        *(.text .text.*)
    }

    .rodata : ALIGN(4K) { *(.rodata .rodata.*) }
    .data   : ALIGN(4K) { *(.data .data.*) }

    . = ALIGN(16);
    __bss_start = .;
    .bss    : ALIGN(16) { *(COMMON) *(.bss .bss.*) }
    . = ALIGN(16);
    __bss_end = .;

    /DISCARD/ : { *(.note.GNU-stack) *(.comment) *(.eh_frame) }
}
