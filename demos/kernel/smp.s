/* demos/kernel/smp.s -- round K5: the sixty lines a second processor
 * cannot be started without.
 *
 * This is the fourth non-Firn file of the kernel and it is here for the
 * same reason as the other three: what stands in it runs BEFORE the
 * machine is in a state a compiled function could run in. An application
 * processor does not wake up in long mode with paging and a stack. It
 * wakes up the way an 8086 woke up in 1978: REAL MODE, 16 bit, cs set to
 * the vector of the startup message and ip at zero, no page tables, no
 * descriptor table it may trust, and 1 MiB of address space. Everything
 * the boot processor got done for it by `boot.s` and the boot loader has
 * to happen again, on the core itself, in the first hundred instructions.
 *
 * WHY THE CODE IS COPIED. The `SIPI` message carries ONE octet of
 * address: the processor starts at `vector << 12`, so the entry point has
 * to lie in the first megabyte and on a page boundary. The kernel image
 * lies at 1 MiB. So the blob between `ap_trampoline` and
 * `ap_trampoline_end` is copied to `AP_BASE` at run time (`smp.fi`), and
 * every address in it is written as "base plus the distance from the
 * start of the blob" -- which is what the `.set ABS_*` lines below do.
 * The alternative, linking this section at 0x8000, would have put a
 * second load segment into the multiboot image for eleven lines of code.
 *
 * WHY THE PARAMETERS LIE IN MEMORY AND NOT IN REGISTERS. A processor
 * started by `SIPI` gets no arguments; the only thing it knows is where
 * it starts. So the boot processor writes what the core needs into a
 * block behind the code -- page table root, its stack, the entry in the
 * kernel, the data area, its number -- and waits until the core has set
 * `P_FLAG`. Only then is the block free for the next core. Bringing them
 * up one at a time costs a few milliseconds and saves the whole question
 * of what happens when two cores read the block at once.
 *
 * WHY THE DESCRIPTOR TABLE HERE HAS THE SAME LAYOUT AS THE ONE IN boot.s
 * (code64 at 0x08, data at 0x10). Not for tidiness: the core stays on
 * this table until Firn code loads the real one, and if the selector
 * numbers differed, the `lgdt` in `smp.ap_main` would leave cs pointing
 * at a different descriptor than the one the processor is executing
 * under. There is no way to reload cs from a compiled function -- that
 * needs a far jump. Keeping the layouts equal removes the need for one.
 */

    .set AP_BASE,  0x8000           /* where the blob is copied to */
    .set AP_PARAM, 0x8F00           /* the parameter block, behind it */

    .set P_CR3,    0x00
    .set P_STACK,  0x08
    .set P_ENTRY,  0x10
    .set P_KDATA,  0x18
    .set P_CPU,    0x20
    .set P_FLAG,   0x28

    .section .text.ap, "ax"
    .code16
    .globl ap_trampoline
ap_trampoline:
    cli
    cld
    xorw %ax, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw %ax, %fs
    movw %ax, %gs

    /* Protected mode, 32 bit. The table is the one at the end of this
     * blob; `ABS_GDTPTR` is its address after the copy. */
    lgdtl ABS_GDTPTR
    movl %cr0, %eax
    orl  $1, %eax
    movl %eax, %cr0
    ljmpl $0x18, $ABS_PROT32

    .code32
ap_prot32:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw %ax, %fs
    movw %ax, %gs

    /* PAE, without which long mode does not exist. */
    movl %cr4, %eax
    orl  $(1 << 5), %eax
    movl %eax, %cr4

    /* The page tables of the boot processor. Not a copy: the SAME ones.
     * Every core walks the same tree, which is what makes the kernel one
     * kernel and not four. */
    movl AP_PARAM + P_CR3, %eax
    movl %eax, %cr3

    /* EFER.LME */
    movl $0xC0000080, %ecx
    rdmsr
    orl  $(1 << 8), %eax
    wrmsr

    /* Paging on -- from this instruction the core is in compatibility
     * mode, and the far jump below makes it 64 bit. */
    movl %cr0, %eax
    orl  $0x80000001, %eax
    movl %eax, %cr0
    ljmp $0x08, $ABS_LONG64

    .code64
ap_long64:
    /* Everything out of the parameter block into registers FIRST, and
     * only then the flag: from the moment the flag is set the boot
     * processor may overwrite the block for the next core. */
    movl $AP_PARAM, %eax
    movq P_STACK(%rax), %rsp
    movq P_KDATA(%rax), %rdi        /* argument 0: the kernel data area */
    movq P_CPU(%rax), %rsi          /* argument 1: which processor this is */
    movq P_ENTRY(%rax), %rdx
    xorl %ebp, %ebp
    movq $1, P_FLAG(%rax)
    call *%rdx                      /* KERNEL_AP_MAIN, and it does not return */
1:
    cli
    hlt
    jmp 1b

    /* The table. Three usable descriptors, and the two the kernel uses
     * afterwards sit at the same selectors as in boot.s. */
    .align 16
ap_gdt:
    .quad 0                         /* 0x00 null */
    .quad 0x00AF9A000000FFFF        /* 0x08 code64  -- as gdt64 */
    .quad 0x00CF92000000FFFF        /* 0x10 data    -- as gdt64 */
    .quad 0x00CF9A000000FFFF        /* 0x18 code32, only for the way up */
ap_gdt_end:
ap_gdt_ptr:
    .word ap_gdt_end - ap_gdt - 1
    .long ABS_GDT
    .globl ap_trampoline_end
ap_trampoline_end:

    /* The addresses the blob has AFTER it was copied. Assembly time
     * constants: every one of them is the distance from the start of the
     * blob plus the place the blob is copied to. */
    .set ABS_GDT,     AP_BASE + (ap_gdt      - ap_trampoline)
    .set ABS_GDTPTR,  AP_BASE + (ap_gdt_ptr  - ap_trampoline)
    .set ABS_PROT32,  AP_BASE + (ap_prot32   - ap_trampoline)
    .set ABS_LONG64,  AP_BASE + (ap_long64   - ap_trampoline)

    /* The table the Firn side reads its two addresses out of. The same
     * mechanism as the `vectors` table of isr.s -- stage 0 of Firn cannot
     * name the address of a symbol in another object file, so the
     * addresses stand in a table whose own address is handed over. */
    .section .rodata
    .align 8
    .globl smp_vectors
smp_vectors:
    .quad ap_trampoline             /* 0: the blob */
    .quad ap_trampoline_end         /* 1: behind it */
    .quad AP_BASE                   /* 2: where it is copied to */
    .quad AP_PARAM                  /* 3: the parameter block */
    .quad gdt64                     /* 4: the descriptor table of boot.s */
