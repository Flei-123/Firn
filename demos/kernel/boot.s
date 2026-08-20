/* demos/kernel/boot.s -- round 59: the boot prologue of the small kernel.
 *
 * This is one of the two non-Firn files of `demos/kernel/kmain.fi` (the
 * other one is `isr.s`). Everything in here is code that a language
 * cannot express, because it runs BEFORE the machine is in the state a
 * compiled function needs:
 *
 *   * the multiboot header (a data structure the boot loader looks for),
 *   * the switch from 32-bit protected mode to LONG MODE (page tables,
 *     CR0/CR3/CR4, EFER, the far jump that reloads CS),
 *   * the GDT, because `lgdt` and the far jump need one before any C-like
 *     code can run,
 *   * the hand-over of the addresses the kernel cannot know otherwise.
 *
 * WHY ADDRESSES ARE HANDED OVER: stage 0 of Firn has no global mutable
 * data (SPEC 14, item 5 -- `const` only). A kernel needs some: the IDT,
 * the frame bitmap, the tick counter. It gets them the way a boot loader
 * hands over a memory region -- as a POINTER in a register. `kernel_main`
 * therefore takes six arguments (System V: rdi, rsi, rdx, rcx, r8, r9):
 *
 *   rdi = multiboot information structure (from the boot loader in ebx)
 *   rsi = `vectors`: the addresses of the entry points out of isr.s
 *   rdx = `kdata`:   the kernel data area (zeroed here, see below)
 *   rcx = `kernel_end`: first byte behind the kernel image
 *   r8  = `gdt64`:   the descriptor table, so that Firn can write the TSS
 *   r9  = `tss`:     the task state segment (RSP0 and IST1)
 *
 * `KERNEL_MAIN` is resolved at link time with `ld --defsym` -- firnc0 puts
 * `_F0.` in front of every symbol, firnc1 `_F1.` (docs/SELF_HOSTING.md).
 *
 * The data area is ZEROED here on purpose: a multiboot loader is not
 * obliged to clear .bss, and a frame bitmap full of random bits allocates
 * random memory.
 */

    .set MB_MAGIC, 0x1BADB002
    .set MB_FLAGS, 0x00000003           /* aligned modules + memory map */
    .set MB_CHECK, -(MB_MAGIC + MB_FLAGS)

    .set KDATA_SIZE, 0x20000            /* 128 KiB, see kstate.fi */

    .section .multiboot, "a"
    .align 4
    .long MB_MAGIC
    .long MB_FLAGS
    .long MB_CHECK

    .section .text.boot, "ax"
    .code32
    .globl _boot
_boot:
    cli
    movl $boot_stack_top, %esp
    /* ebx holds the multiboot information; nothing below touches it. */

    /* --- clear the page tables (3 * 4 KiB) */
    movl $pml4, %edi
    xorl %eax, %eax
    movl $(3 * 4096 / 4), %ecx
    rep stosl

    /* --- clear the kernel data area (the loader does not have to) */
    movl $kdata, %edi
    xorl %eax, %eax
    movl $(KDATA_SIZE / 4), %ecx
    rep stosl

    /* --- PML4[0] -> PDPT, PDPT[0] -> PD  (present | writable) */
    movl $pdpt, %eax
    orl  $0x3, %eax
    movl %eax, pml4

    movl $pd, %eax
    orl  $0x3, %eax
    movl %eax, pdpt

    /* --- PD: 512 entries of 2 MiB, identity mapped
     *     (present | writable | huge page) */
    movl $pd, %edi
    movl $0x83, %eax
    movl $512, %ecx
1:
    movl %eax, (%edi)
    movl $0, 4(%edi)
    addl $0x200000, %eax
    addl $8, %edi
    loop 1b

    /* --- enable PAE (CR4.PAE) */
    movl %cr4, %eax
    orl  $(1 << 5), %eax
    movl %eax, %cr4

    /* --- CR3 to the PML4 */
    movl $pml4, %eax
    movl %eax, %cr3

    /* --- enable long mode (EFER.LME, MSR 0xC0000080) */
    movl $0xC0000080, %ecx
    rdmsr
    orl  $(1 << 8), %eax
    wrmsr

    /* --- paging on (CR0.PG | CR0.PE) */
    movl %cr0, %eax
    orl  $0x80000001, %eax
    movl %eax, %cr0

    /* --- load the 64-bit GDT and jump into long mode */
    lgdt gdt64_pointer
    ljmp $0x08, $long_mode

    .code64
long_mode:
    /* Data segments are meaningless in long mode, but cleanly zeroed. */
    xorw %ax, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw %ax, %fs
    movw %ax, %gs

    movq $kernel_stack_top, %rsp
    xorq %rbp, %rbp

    movl %ebx, %edi                     /* multiboot info (zero extended) */
    movq $vectors, %rsi
    movq $kdata, %rdx
    movq $kernel_end, %rcx
    movq $gdt64, %r8
    movq $tss, %r9
    call KERNEL_MAIN                      /* does not return */
2:
    hlt
    jmp 2b

    /* --------------------------------------------------------- GDT ---
     *
     * The layout is the one `syscall`/`sysret` prescribes: STAR[63:48]
     * points at 0x18, `sysretq` takes SS from 0x18+8 = 0x20 and CS from
     * 0x18+16 = 0x28. That is why the unused 32-bit user code segment
     * sits at 0x18 -- not out of nostalgia but because the processor
     * counts from there.
     *
     * The table lies in .data and not in .rodata: the kernel writes the
     * TSS descriptor (0x30/0x38) at run time, out of Firn.
     */
    .section .data
    .align 16
    .globl gdt64
gdt64:
    .quad 0                             /* 0x00 null */
    .quad 0x00AF9A000000FFFF            /* 0x08 kernel code64 */
    .quad 0x00CF92000000FFFF            /* 0x10 kernel data */
    .quad 0x00CFFA000000FFFF            /* 0x18 user code32 (STAR base) */
    .quad 0x00CFF2000000FFFF            /* 0x20 user data, DPL 3 */
    .quad 0x00AFFA000000FFFF            /* 0x28 user code64, DPL 3 */
    .quad 0                             /* 0x30 TSS low, written by Firn */
    .quad 0                             /* 0x38 TSS high, written by Firn */
gdt64_end:
    .globl gdt64_pointer
gdt64_pointer:
    .word gdt64_end - gdt64 - 1
    .quad gdt64

    /* --------------------------- page tables, stacks, data area --- */
    .section .bss, "aw", @nobits
    .align 4096
pml4:
    .skip 4096
pdpt:
    .skip 4096
pd:
    .skip 4096

    /* The task state segment. 104 bytes; RSP0 (+4) and IST1 (+36) are
     * written by the kernel in Firn, the I/O map base (+100) as well. */
    .align 16
    .globl tss
tss:
    .skip 104

    .align 16
boot_stack_bottom:
    .skip 16384
boot_stack_top:

    .align 16
kernel_stack_bottom:
    .skip 65536
    .globl kernel_stack_top
kernel_stack_top:

    /* Own stacks for the double fault (IST1) and for `syscall`: a fault
     * whose cause is a broken rsp cannot be reported on that same rsp. */
    .align 16
df_stack_bottom:
    .skip 16384
    .globl df_stack_top
df_stack_top:

    .align 16
syscall_stack_bottom:
    .skip 16384
    .globl syscall_stack_top
syscall_stack_top:

    /* RSP0 in the TSS: the stack an interrupt out of ring 3 lands on. It
     * has to be one of its own -- the kernel stack still carries the
     * frames of the function that entered ring 3, and an interrupt would
     * write over them. */
    .align 16
irq_stack_bottom:
    .skip 16384
    .globl irq_stack_top
irq_stack_top:

    /* The kernel data area: state block, IDT, frame bitmap, heap
     * metadata. The division is in `demos/kernel/kstate.fi`. */
    .align 4096
    .globl kdata
kdata:
    .skip KDATA_SIZE
