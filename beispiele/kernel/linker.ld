/* beispiele/kernel/linker.ld — Runde 52.
 *
 * Bindet `start.s` (Multiboot-Kopf, Langer Modus) und die freistehende
 * Objektdatei aus `kern.fi` zu einem Abbild, das `qemu-system-x86_64 -kernel`
 * laedt. KEIN libc, keine Startdateien (crt*.o), kein dynamischer Binder.
 *
 * Der Multiboot-Kopf muss in den ersten 8 KiB liegen — deshalb steht er
 * zuerst, unmittelbar gefolgt vom 32-Bit-Vorspann.
 *
 * `KERN_START` setzt `run.sh` beim Binden (`ld --defsym`): firnc0 stellt
 * jedem Symbol `_F0.` voran, firnc1 `_F1.` (docs/SELBSTHOSTING.md).
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

    .rodata : ALIGN(4K) { *(.rodata .rodata.*) }
    .data   : ALIGN(4K) { *(.data .data.*) }
    .bss    : ALIGN(4K) { *(COMMON) *(.bss .bss.*) }

    /DISCARD/ : { *(.note.GNU-stack) *(.comment) *(.eh_frame) }
}
