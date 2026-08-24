/* demos/kernel/user/crt.s -- round K1: the four instructions a user
 * program cannot write in Firn.
 *
 * Everything else down here is Firn. What is left over is exactly what a
 * language has no words for:
 *
 *   1. THE ENTRY POINT. The kernel jumps to `_start` with `rdi` pointing
 *      at the argument block it built on the top stack page (see
 *      `demos/kernel/elf.fi`), and with a stack pointer that is 16
 *      aligned. System V wants `rsp % 16 == 8` INSIDE a function -- the
 *      `call` below is what produces that, and a Firn function cannot
 *      emit a `call` to itself.
 *   2. THE WAY OUT. A Firn `fn` ends in `ret`, and there is nothing to
 *      return to: under `_start` lies no frame. The return value is the
 *      exit code, and `exit` is a system call.
 *   3. `osum_panic`. Under `profile kernel` a checked arithmetic site
 *      that goes out of range calls this name (SPEC section 13, item L9);
 *      the compiler leaves it undefined on purpose. In the kernel
 *      `isr.s` defines it. A program on the disk has no kernel to fall
 *      back on, so it defines its own: say what happened on descriptor 2
 *      and leave with 70. Without this definition every user program
 *      would carry an undefined symbol, and `ld` would refuse it.
 *
 * The system call numbers are the ones in `demos/kernel/sys.fi`:
 * 10 = exit, 11 = write.
 */

    .section .text
    .globl _start
_start:
    xorq %rbp, %rbp                 /* no caller frame under us */
    andq $-16, %rsp                 /* System V: aligned before the call */
    call USER_ENTRY                 /* rdi already holds the arguments */
    movq %rax, %rdi                 /* the return value is the exit code */
    movq $10, %rax
    syscall
1:  jmp 1b                          /* exit does not come back */

    .globl osum_panic
osum_panic:
    /* rdi = message, esi = length, rdx/rcx = the two operands. */
    movq %rsi, %rdx
    movq %rdi, %rsi
    movq $2, %rdi                   /* descriptor 2 */
    movq $11, %rax                  /* write */
    syscall
    movq $70, %rdi                  /* and out, with a code of its own */
    movq $10, %rax
    syscall
2:  jmp 2b

    .section .note.GNU-stack,"",@progbits
