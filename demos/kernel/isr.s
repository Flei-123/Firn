/* demos/kernel/isr.s -- round 59: the interrupt entry points.
 *
 * The second and last non-Firn file of the kernel. `#[interrupt]` in Firn
 * (round 52) produces a handler that saves all registers and ends with
 * `iretq` -- but it cannot do three things a real kernel needs:
 *
 *   1. it does not know WHICH vector it was entered through,
 *   2. it cannot read the ERROR CODE that the processor pushes for ten of
 *      the exceptions (and only for those),
 *   3. it cannot pass a POINTER to the saved registers on, so that a
 *      handler in Firn could print or change them.
 *
 * Hence 48 stubs of three instructions each. They equalise the two shapes
 * of the exception frame (with and without error code), push the vector
 * number and jump into ONE common stub. That one saves all 15 general
 * purpose registers, hands `rsp` (= the address of the frame) and the
 * kernel data area to `KERNEL_TRAP` -- and everything after that happens in
 * Firn.
 *
 * The frame that `KERNEL_TRAP` sees, offsets from rdi:
 *
 *     +0 r15  +8 r14  +16 r13  +24 r12  +32 r11  +40 r10  +48 r9
 *     +56 r8  +64 rdi +72 rsi  +80 rbp  +88 rbx  +96 rdx  +104 rcx
 *     +112 rax  +120 vector  +128 error code
 *     +136 rip  +144 cs  +152 rflags  +160 rsp  +168 ss
 *
 * The last five are what the processor itself pushed.
 *
 * `KERNEL_TRAP`, `KERNEL_SYSCALL` and `KERNEL_MAIN` are resolved at link time
 * with `ld --defsym`; the symbol prefix differs between the two compilers
 * (`_F0.` / `_F1.`).
 */

    .section .text

/* A vector WITHOUT an error code: push a zero, so that every frame has
 * the same shape. */
    .macro isr_plain num
    .globl isr\num
isr\num:
    pushq $0
    pushq $\num
    jmp isr_common
    .endm

/* A vector WITH an error code: the processor has already pushed it. */
    .macro isr_code num
    .globl isr\num
isr\num:
    pushq $\num
    jmp isr_common
    .endm

    isr_plain 0                 /* #DE divide error */
    isr_plain 1
    isr_plain 2
    isr_plain 3
    isr_plain 4
    isr_plain 5
    isr_plain 6                 /* #UD invalid opcode */
    isr_plain 7
    isr_code  8                 /* #DF double fault */
    isr_plain 9
    isr_code  10
    isr_code  11
    isr_code  12                /* #SS stack fault */
    isr_code  13                /* #GP general protection */
    isr_code  14                /* #PF page fault */
    isr_plain 15
    isr_plain 16
    isr_code  17
    isr_plain 18
    isr_plain 19
    isr_plain 20
    isr_code  21
    isr_plain 22
    isr_plain 23
    isr_plain 24
    isr_plain 25
    isr_plain 26
    isr_plain 27
    isr_plain 28
    isr_code  29
    isr_code  30
    isr_plain 31
    isr_plain 32                /* IRQ0  timer */
    isr_plain 33                /* IRQ1  keyboard */
    isr_plain 34
    isr_plain 35
    isr_plain 36
    isr_plain 37
    isr_plain 38
    isr_plain 39
    isr_plain 40
    isr_plain 41
    isr_plain 42
    isr_plain 43
    isr_plain 44
    isr_plain 45
    isr_plain 46
    isr_plain 47

isr_common:
    pushq %rax
    pushq %rcx
    pushq %rdx
    pushq %rbx
    pushq %rbp
    pushq %rsi
    pushq %rdi
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r11
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15

    movq %rsp, %rdi                 /* the frame */
    movq $kdata, %rsi               /* the kernel data area */
    movq %rsp, %rbx                 /* rbx is saved, so it may be used */
    andq $-16, %rsp                 /* System V wants a 16-aligned stack */
    call KERNEL_TRAP
    movq %rbx, %rsp

    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %r11
    popq %r10
    popq %r9
    popq %r8
    popq %rdi
    popq %rsi
    popq %rbp
    popq %rbx
    popq %rdx
    popq %rcx
    popq %rax
    addq $16, %rsp                  /* vector number and error code */
    iretq

/* ------------------------------------------------------ system call ---
 *
 * `syscall` changes neither rsp nor the stack: the processor only puts
 * the return address into rcx and rflags into r11. Switching to a kernel
 * stack is the kernel's job -- and cannot be done in a compiled function,
 * because that one would already have written to the user stack.
 *
 * Handed to Firn: rdi = number (came in rax), rsi = argument (came in
 * rdi), rdx = kernel data area. The result of `KERNEL_SYSCALL` goes back to
 * the user in rax.
 */
    .globl syscall_entry
syscall_entry:
    movq %rsp, %r10                 /* r10 is scratch under this ABI */
    movq $syscall_stack_top, %rsp
    pushq %r10                      /* user rsp */
    pushq %rcx                      /* user rip */
    pushq %r11                      /* user rflags */
    subq $8, %rsp                   /* keep the stack 16-aligned */

    movq %rdi, %rsi
    movq %rax, %rdi
    movq $kdata, %rdx
    call KERNEL_SYSCALL

    addq $8, %rsp
    popq %r11
    popq %rcx
    popq %r10
    movq %r10, %rsp
    sysretq

/* ------------------------------------------------- ring 3 and back ---
 *
 * `enter_user(rip, rsp)` leaves ring 0 through `sysretq`; `leave_user()`
 * comes back. The return happens without `sysret`, because the way back
 * is not a return to the user program but the abandonment of it: the
 * kernel stack pointer saved in `enter_user` is put back, and `ret` then
 * returns to whoever called `enter_user`. Registers that System V calls
 * callee-saved are rescued along -- the caller is a compiled Firn
 * function and relies on them.
 */
    .globl enter_user
enter_user:
    movq %rsp, saved_rsp(%rip)
    movq %rbx, saved_rbx(%rip)
    movq %rbp, saved_rbp(%rip)
    movq %r12, saved_r12(%rip)
    movq %r13, saved_r13(%rip)
    movq %r14, saved_r14(%rip)
    movq %r15, saved_r15(%rip)
    movq %rdi, %rcx                 /* sysret takes the rip out of rcx */
    movq $0x202, %r11               /* rflags: reserved bit + IF */
    movq %rsi, %rsp                 /* the user stack */
    sysretq

    .globl leave_user
leave_user:
    movq saved_rsp(%rip), %rsp
    movq saved_rbx(%rip), %rbx
    movq saved_rbp(%rip), %rbp
    movq saved_r12(%rip), %r12
    movq saved_r13(%rip), %r13
    movq saved_r14(%rip), %r14
    movq saved_r15(%rip), %r15
    xorl %eax, %eax
    ret

/* The user program. Deliberately not in Firn: everything that runs here
 * has to lie in a page mapped for ring 3 -- the linker script places
 * exactly this section on its own 4 KiB page, and the kernel sets the
 * user bit for that page and no other. A compiled Firn function would sit
 * in the middle of the kernel's text.
 *
 * Number 1 = write a mark, number 2 = leave ring 3.
 *
 * THE COUNTER-CHECK is the branch: when the kernel answers system call 1
 * with something other than zero, the program executes `hlt`. That
 * instruction is allowed in ring 0 and forbidden in ring 3 -- if a #GP
 * with cs=0x2b arrives afterwards, the processor really was in ring 3.
 * Without such a probe, "we were in ring 3" would be a claim.
 */
    .section .utext, "ax"
    .globl user_entry
user_entry:
    movq $1, %rax
    movq $59, %rdi
    syscall
    testq %rax, %rax
    jnz 2f
    movq $2, %rax
    xorq %rdi, %rdi
    syscall
1:
    jmp 1b
2:
    hlt                             /* privileged: #GP in ring 3 */
    jmp 1b

    .section .rodata
    .align 8
    .globl vectors
vectors:
    .quad isr0,  isr1,  isr2,  isr3,  isr4,  isr5,  isr6,  isr7
    .quad isr8,  isr9,  isr10, isr11, isr12, isr13, isr14, isr15
    .quad isr16, isr17, isr18, isr19, isr20, isr21, isr22, isr23
    .quad isr24, isr25, isr26, isr27, isr28, isr29, isr30, isr31
    .quad isr32, isr33, isr34, isr35, isr36, isr37, isr38, isr39
    .quad isr40, isr41, isr42, isr43, isr44, isr45, isr46, isr47
    .quad syscall_entry             /* 48 */
    .quad user_entry                /* 49 */
    .quad enter_user                /* 50 */
    .quad leave_user                /* 51 */
    .quad df_stack_top              /* 52: IST1, for the double fault */
    .quad irq_stack_top             /* 53: RSP0, for interrupts out of ring 3 */
    .quad kernel_stack_top          /* 54 */

    .section .bss, "aw", @nobits
    .align 8
saved_rsp:
    .skip 8
saved_rbx:
    .skip 8
saved_rbp:
    .skip 8
saved_r12:
    .skip 8
saved_r13:
    .skip 8
saved_r14:
    .skip 8
saved_r15:
    .skip 8
