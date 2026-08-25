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
 * ROUND 62 changes two things here. There is more than one process now,
 * so the kernel stack is no longer a single fixed one: the scheduler
 * writes the stack of the CURRENT task into the data area, and the entry
 * point reads it from there (`KSTACK_CUR`, see kstate.fi). A task that is
 * in ring 3 has an empty kernel stack -- that is why its upper end may be
 * taken without further ado. And a system call now carries three
 * arguments instead of one.
 *
 * The ABI, seen from ring 3:
 *
 *     rax = number, rdi = arg0, rsi = arg1, rdx = arg2
 *     rax = result (negative = error code, see sys.fi)
 *     rcx, r11 destroyed by the processor, r8/r10 by the kernel
 *
 * Handed to Firn: rdi = number, rsi/rdx/rcx = the three arguments,
 * r8 = kernel data area.
 */
    .set KSTACK_CUR, 344            /* kstate.KSTACK_CUR -- kstate.fi */

    .globl syscall_entry
syscall_entry:
    /* ROUND K4. Two things changed here, and both are the reason this
     * round exists.
     *
     * FIRST: r10 is NOT scratch any more. Linux x86-64 passes the fourth
     * argument of a system call in r10 (`syscall` destroys rcx, so the C
     * argument in rcx moves one register aside), the fifth in r8 and the
     * sixth in r9. `mmap` has six of them. The old entry point wrote the
     * user stack pointer over r10 before anybody could look at it. It goes
     * into `sys_rsp` instead -- interrupts are off between the store and
     * the push that reads it back, because MSR_SFMASK masks IF for the
     * whole system call (`demos/kernel/user.fi`, `setup`).
     *
     * SECOND: what is saved is a COMPLETE user context -- sixteen words on
     * the kernel stack of the calling task -- and its ADDRESS is what the
     * kernel gets. Three calls need that and cannot be written without it:
     *
     *   * `fork` gives the child the register set of the parent, rbx, rbp
     *     and r12..r15 included -- registers a `call` preserves and that
     *     therefore never appear in a Firn argument,
     *   * `execve` returns to a DIFFERENT rip with a DIFFERENT rsp in the
     *     SAME task: it writes both into the frame and lets the ordinary
     *     way out run,
     *   * a signal handler, when there is one, needs exactly the same.
     *
     * The layout, as offsets from the pointer Firn receives (the F_*
     * constants at the top of `demos/kernel/sys.fi`):
     *
     *   +0   r15  +8  r14  +16 r13  +24 r12  +32 rbp  +40 rbx
     *   +48  r9   +56 r8   +64 r10  +72 rdx  +80 rsi  +88 rdi
     *   +96  rax (the number on the way in, the result on the way out)
     *   +104 rip  +112 rflags  +120 user rsp
     *
     * Sixteen words are 128 octets, so the 16-alignment of the kernel
     * stack survives them and the `call` below gets what System V wants.
     *
     * Handed to Firn: rdi = the number, rsi = the frame, rdx = the kernel
     * data area.
     */
    movq %rsp, sys_rsp(%rip)        /* the user stack -- no register is free */
    movq kdata + KSTACK_CUR(%rip), %rsp
    pushq sys_rsp(%rip)             /* user rsp */
    pushq %r11                      /* user rflags */
    pushq %rcx                      /* user rip */
    pushq %rax                      /* the number */
    pushq %rdi                      /* argument 0 */
    pushq %rsi                      /* argument 1 */
    pushq %rdx                      /* argument 2 */
    pushq %r10                      /* argument 3 */
    pushq %r8                       /* argument 4 */
    pushq %r9                       /* argument 5 */
    pushq %rbx
    pushq %rbp
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15

    movq %rax, %rdi                 /* the number */
    movq %rsp, %rsi                 /* the frame */
    movq $kdata, %rdx               /* the data area */
    call KERNEL_SYSCALL
    movq %rax, 96(%rsp)             /* the result belongs in the frame */
    jmp user_return

/* `user_resume(rdi = frame)` -- a context that does NOT lie on the stack
 * this processor is standing on. `proc.resume_start` starts the child of a
 * `fork` with it: the frame was copied into the data area, the rax in it
 * is zero, and from here the child is indistinguishable from a process
 * that has just come back out of a system call. `cli`, because rsp walks
 * through that copy for sixteen instructions and an interrupt would push
 * its own frame into the middle of it; `sysretq` puts the flags back out
 * of r11, IF included.
 */
    .globl user_resume
user_resume:
    cli
    movq %rdi, %rsp
user_return:
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %rbp
    popq %rbx
    popq %r9
    popq %r8
    popq %r10
    popq %rdx
    popq %rsi
    popq %rdi
    popq %rax
    popq %rcx                       /* the rip `sysret` returns to */
    popq %r11                       /* the flags it puts back */
    popq %rsp                       /* the user stack */
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
    /* Round 62: the scheduler, the processes and the user programs. */
    .quad context_switch            /* 55: switch.s */
    .quad enter_user_task           /* 56: switch.s, into ring 3 for good */
    .quad task_guard                /* 57: switch.s, under every new task */
    .quad USER_MAIN                 /* 58: entry point of uprog.fi */
    .quad __user_begin              /* 59: first page of the user programs */
    .quad __user_end                /* 60: behind their last page */
    .quad syscall_stack_top         /* 61: the stack before the first task */
    .quad KERNEL_TASK_MAIN          /* 62: tasks.fi, the body of a kernel task */
    .quad KERNEL_USER_START         /* 63: proc.fi, the way into ring 3 */
    /* Round K4: back into ring 3 with a context the kernel wrote itself. */
    .quad user_resume               /* 64: fork and execve */
    /* Round K5: the other processors (demos/kernel/smp.s, smp.fi). */
    .quad smp_vectors               /* 65: the table of the trampoline */
    .quad KERNEL_AP_MAIN            /* 66: where a started core lands */

    .section .bss, "aw", @nobits
    .align 8
saved_rsp:
    .skip 8
/* Round K4: where `syscall_entry` parks the user stack pointer for the two
 * instructions before it can push it. One processor, and IF masked by
 * MSR_SFMASK -- nothing can get in between. */
sys_rsp:
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

    .text
    /* --------------------------------------------------- osum_panic ---
     * ROUND 72: the kernel's own Firn code (`fs.fi::fs__mount`, `uprog.fi::
     * u_mkdir`) now uses CHECKED arithmetic (SPEC section 13, `L9`) --
     * under `profile kernel` a checked site that goes out of range ends in
     * `call osum_panic`, an EXTERNAL symbol the compiler deliberately
     * leaves undefined (SPEC section 2: "calls osum_panic, configurable").
     * This is that definition, same shape as `demos/kernel/start.s`'s
     * (the smaller kernel demo): write the message to COM1 -- already
     * initialised by `kmain.fi`'s own `serial.init()`, which runs before
     * anything that could trigger a checked panic -- then halt for good.
     *
     * ABI at the call site (panic_rt.rs::trampoline_asm, kernel branch):
     *   rdi = message pointer, esi = message length, rdx/rcx = the two
     *   operand values, r8 = panic kind code (unused here, see start.s's
     *   own comment on why: decimal formatting is `app`-profile-only
     *   machinery this is not worth duplicating for a two-line message).
     */
    .globl osum_panic
osum_panic:
    movq %rdi, %r10
    movl %esi, %r11d
.Losum_loop:
    testl %r11d, %r11d
    jz .Losum_nl
    movw $0x3FD, %dx
.Losum_wait:
    inb %dx, %al
    testb $0x20, %al
    jz .Losum_wait
    movb (%r10), %al
    movw $0x3F8, %dx
    outb %al, %dx
    incq %r10
    decl %r11d
    jmp .Losum_loop
.Losum_nl:
    movb $10, %al
    movw $0x3F8, %dx
    outb %al, %dx
.Losum_halt:
    cli
    hlt
    jmp .Losum_halt
