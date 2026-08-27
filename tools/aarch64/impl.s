/* SPDX-License-Identifier: GPL-2.0-only */
// tools/aarch64/impl.s -- the A64 twin of tools/extfn/impl.s: a
// hand-written `strlen`, AAPCS64 (x0 = the pointer, x0 = the length up to
// but not including the first zero byte). Deliberately NOT libc, so
// direction 1 of the foreign-function proof does not secretly lean on the
// system's C library.
.text
.globl strlen
strlen:
    mov x1, #0
.Lstrlen_loop:
    ldrb w2, [x0, x1]
    cbz w2, .Lstrlen_done
    add x1, x1, #1
    b .Lstrlen_loop
.Lstrlen_done:
    mov x0, x1
    ret
.section .note.GNU-stack,"",%progbits
