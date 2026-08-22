/* demos/kernel/start.s — der einzige Nicht-Firn-Code des Kernels.
 *
 * Aufgabe: den Rechner aus dem 32-Bit-Zustand, in dem ein
 * Multiboot-Lader (QEMU `-kernel`) uebergibt, in den LANGEN MODUS bringen
 * und dann `KERN_START` aufrufen — das ist `kern_start` aus core.fi,
 * dessen Symbolpraefix `run.sh` beim Binden per `--defsym` einsetzt
 * (firnc0: `_F0.`, firnc1: `_F1.`).
 *
 * Seitentabellen werden ZUR LAUFZEIT gebaut und vorher genullt — auf den
 * .bss-Inhalt eines Multiboot-Laders wird sich hier bewusst nicht
 * verlassen. Abgebildet wird das erste Gigabyte mit 2-MiB-Seiten
 * (identisch), das genuegt fuer Code, Stapel, VGA (0xB8000) und COM1.
 */

    .set MB_MAGIC,  0x1BADB002
    .set MB_FLAGS,  0x00000003          /* ausgerichtet + Speicherkarte */
    .set MB_PRUEF,  -(MB_MAGIC + MB_FLAGS)

    .section .multiboot, "a"
    .align 4
    .long MB_MAGIC
    .long MB_FLAGS
    .long MB_PRUEF

    .section .text.boot, "ax"
    .code32
    .globl _boot
_boot:
    cli
    movl $stapel_oben, %esp

    /* --- Seitentabellen nullen (3 * 4 KiB) */
    movl $pml4, %edi
    movl %edi, %ecx
    xorl %eax, %eax
    movl $(3 * 4096 / 4), %ecx
    rep stosl

    /* --- PML4[0] -> PDPT, PDPT[0] -> PD  (present | writable) */
    movl $pdpt, %eax
    orl  $0x3, %eax
    movl %eax, pml4

    movl $pd, %eax
    orl  $0x3, %eax
    movl %eax, pdpt

    /* --- PD: 512 Eintraege a 2 MiB, identisch abgebildet
     *     (present | writable | grosse Seite) */
    movl $pd, %edi
    movl $0x83, %eax                    /* physische Adresse 0, Flags */
    movl $512, %ecx
1:
    movl %eax, (%edi)
    movl $0, 4(%edi)
    addl $0x200000, %eax
    addl $8, %edi
    loop 1b

    /* --- PAE einschalten (CR4.PAE) */
    movl %cr4, %eax
    orl  $(1 << 5), %eax
    movl %eax, %cr4

    /* --- CR3 auf die PML4 */
    movl $pml4, %eax
    movl %eax, %cr3

    /* --- Langen Modus freischalten (EFER.LME, MSR 0xC0000080) */
    movl $0xC0000080, %ecx
    rdmsr
    orl  $(1 << 8), %eax
    wrmsr

    /* --- Seitenverwaltung an (CR0.PG | CR0.PE) */
    movl %cr0, %eax
    orl  $0x80000001, %eax
    movl %eax, %cr0

    /* --- 64-Bit-GDT laden und in den langen Modus springen */
    lgdt gdt64_zeiger
    ljmp $0x08, $lang

    .code64
lang:
    /* Datensegmente im langen Modus bedeutungslos, aber sauber genullt */
    xorw %ax, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw %ax, %fs
    movw %ax, %gs

    movq $stapel_oben, %rsp
    xorq %rbp, %rbp

    call KERN_START                     /* kehrt nicht zurueck */
2:
    hlt
    jmp 2b

    /* --------------------------------------------------- karst_panic ---
     * ROUND 72: `demos/kernel/core.fi` now uses CHECKED arithmetic
     * (`old + 1 as u16` in `timer_ih`) -- the whole point of round 72 is
     * that the checked build levels catch an out-of-range value instead
     * of wrapping past it silently. Under `profile kernel`
     * (`panic_rt.rs::trampoline_asm`) that check ends in a `call
     * karst_panic`, an EXTERNAL symbol the kernel author must define --
     * intentionally left undefined by the compiler itself (SPEC section 2:
     * "calls karst_panic, configurable"). This is that definition: the
     * smallest one that is still HONEST about what happened, not a
     * silent `ret` back into code that already proved its own value was
     * wrong.
     *
     * ABI at the call site (panic_rt.rs::trampoline_asm, kernel branch):
     *   rdi = message pointer (into .rodata, no NUL terminator)
     *   esi = message length
     *   rdx = first operand's value (sign extended to 64 bits)
     *   rcx = second operand's value (same value twice for a cast, see
     *         panic_rt.rs's own header comment)
     *   r8  = panic kind code (PANIC_ADD=1 .. PANIC_CAST=6, panic_rt.rs)
     *
     * What it does: write the message text to COM1 (already initialised
     * by `core.fi::serial_init`, which always runs before any interrupt
     * this demo installs can fire), THEN halt for good -- a kernel that
     * just found its own arithmetic out of range has no safe way to
     * continue, and guessing one back here would defeat the entire point
     * of checking in the first place.
     */
    .globl karst_panic
karst_panic:
    /* rdi/esi (message pointer/length) are exactly what `out8`'s COM1
     * write loop below needs; a/b/code (rdx/rcx/r8) are not printed here
     * -- decimal formatting is `panic_rt.rs`'s own hand-rolled routine
     * (`.Lpanic_i64_dec`, `app` profile only) and duplicating it here in
     * hand-written assembly for a two line demo kernel is not worth
     * doing twice; the message text alone already names the operator,
     * the file and the line. */
    movq %rdi, %r10                     /* r10 = cursor into the message */
    movl %esi, %r11d                    /* r11d = remaining byte count */
3:
    testl %r11d, %r11d
    jz 5f
    /* wait for the transmit holding register to be empty (LSR bit 5) */
    movw $0x3FD, %dx                    /* COM1 (0x3F8) + 5 = line status */
4:
    inb %dx, %al
    testb $0x20, %al
    jz 4b
    movb (%r10), %al
    movw $0x3F8, %dx                    /* COM1 data register */
    outb %al, %dx
    incq %r10
    decl %r11d
    jmp 3b
5:
    /* a trailing newline, so the last line of the message is not left
     * sitting in whatever the terminal buffers next */
    movb $10, %al
    movw $0x3F8, %dx
    outb %al, %dx
6:
    cli
    hlt
    jmp 6b

    /* ------------------------------------------------------------- GDT */
    .section .rodata
    .align 16
gdt64:
    .quad 0                             /* Nulldeskriptor */
    /* Code64: ausfuehrbar, present, Codesegment, L-Bit */
    .quad 0x00AF9A000000FFFF
    /* Data: schreibbar, present */
    .quad 0x00CF92000000FFFF
gdt64_ende:
gdt64_zeiger:
    .word gdt64_ende - gdt64 - 1
    .quad gdt64

    /* ------------------------------------------ Seitentabellen + Stapel */
    .section .bss, "aw", @nobits
    .align 4096
pml4:
    .skip 4096
pdpt:
    .skip 4096
pd:
    .skip 4096
    .align 16
stapel_unten:
    .skip 16384
stapel_oben:
