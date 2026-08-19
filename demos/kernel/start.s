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
