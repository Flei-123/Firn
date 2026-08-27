/* SPDX-License-Identifier: GPL-2.0-only */
// tools/extfn/impl.s -- a hand-written 'strlen', System V AMD64: rdi = the
// pointer, returns rax = the length up to (not including) the first zero
// byte. Deliberately NOT libc, so direction 1 does not secretly depend on
// the system's C library -- the object file is self-contained.
.intel_syntax noprefix
.text
.globl strlen
strlen:
    xor rax, rax
.strlen_loop:
    cmp byte ptr [rdi + rax], 0
    je .strlen_done
    inc rax
    jmp .strlen_loop
.strlen_done:
    ret
.section .note.GNU-stack,"",@progbits
