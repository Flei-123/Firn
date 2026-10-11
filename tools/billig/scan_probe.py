#!/usr/bin/env python3
"""ROUND BILLIG -- find out WHICH stack word keeps an object alive (aarch64).

The collector scans the stack conservatively, so "this object should be dead"
is a question about every word of every live frame. This tool answers it on
the real binary without changing the program: it replaces `__gc_scan` in the
assembler text by a version that, for every word it looks at, reports

    0xBB lo hi                      one line per scan (start and end address)
    0xCC fp lr                      the frame chain of the scanning thread
    0xAA dist addr word tid         every word that points into a live heap
                                    block: distance below the scan's end, the
                                    address of the word, the pointer, and the
                                    type id of the block it hits

on standard error (hex). Map `addr` onto the frame chain to see which frame
(and which slot of it) holds the word, and `tid` onto the type table to see
what it keeps alive (`.L__gc_typetable.n<tid>` names the class).

How it was used (tests/843_collections_interplay.fi, aarch64 only): the
`Handle` stayed alive after `gcvec_clear`, and the word sat at fp-296 of
`gc_collect` -- a frame slot that no instruction of `gc_collect` ever wrote
(the allocator keeps the value in a register, the slot was only reserved).

    firnc --target=aarch64-linux --emit=asm -o prog.s prog.fi
    python3 tools/billig/scan_probe.py prog.s probe.s
    aarch64-linux-gnu-as probe.s -o probe.o
    aarch64-linux-gnu-ld -n -o probe probe.o
    qemu-aarch64 ./probe 2> scan.log

usage: scan_probe.py in.s out.s
"""
import re, sys

src = open(sys.argv[1]).read()
start = src.index(".globl _F0.__gc_scan\n_F0.__gc_scan:")
end = src.index(".globl _F0.__gc_sweep_init", start)

new_scan = r"""
.globl _F0.__gc_scan
_F0.__gc_scan:
    stp x29, x30, [sp, #-64]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    stp x21, x22, [sp, #32]
    str x23, [sp, #48]
    add x19, x0, #7
    and x19, x19, #0xfffffffffffffff8
    mov x20, x1
    mov x21, x0
    movz x0, #0xBB
    bl __dbg_hex
    mov x0, x21
    bl __dbg_hex
    mov x0, x20
    bl __dbg_hex
    bl __dbg_nl
    // frame chain: fp, saved lr per frame
    mov x22, x29
    mov x23, #0
.Ldbg_chain:
    cbz x22, .Ldbg_chain_end
    cmp x23, #10
    b.hs .Ldbg_chain_end
    movz x0, #0xCC
    bl __dbg_hex
    mov x0, x22
    bl __dbg_hex
    ldr x0, [x22, #8]
    bl __dbg_hex
    bl __dbg_nl
    ldr x22, [x22]
    add x23, x23, #1
    b .Ldbg_chain
.Ldbg_chain_end:
.Ldbg_scan_loop:
    add x2, x19, #8
    cmp x2, x20
    b.hi .Ldbg_scan_end
    ldr x22, [x19]
    mov x0, x19
    mov x1, x22
    mov x2, x20
    bl __dbg_hit
    mov x0, x22
    bl _F0.__gc_mark
    add x19, x19, #8
    b .Ldbg_scan_loop
.Ldbg_scan_end:
    ldp x19, x20, [sp, #16]
    ldp x21, x22, [sp, #32]
    ldr x23, [sp, #48]
    ldp x29, x30, [sp], #64
    ret

"""

helpers = r"""
.globl __dbg_hex
__dbg_hex:
    stp x29, x30, [sp, #-48]!
    mov x29, sp
    mov x2, x0
    mov x3, #0
    add x4, sp, #16
.Ldbg_hex_loop:
    mov x5, #60
    lsl x6, x3, #2
    sub x5, x5, x6
    lsr x6, x2, x5
    and x6, x6, #15
    cmp x6, #10
    b.lo .Ldbg_hex_digit
    add x6, x6, #87
    b .Ldbg_hex_store
.Ldbg_hex_digit:
    add x6, x6, #48
.Ldbg_hex_store:
    strb w6, [x4, x3]
    add x3, x3, #1
    cmp x3, #16
    b.lo .Ldbg_hex_loop
    mov w6, #32
    strb w6, [x4, #16]
    mov x0, #2
    mov x1, x4
    mov x2, #17
    mov x8, #64
    svc #0
    ldp x29, x30, [sp], #48
    ret

.globl __dbg_nl
__dbg_nl:
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    mov w1, #10
    strb w1, [sp, #16]
    mov x0, #2
    add x1, sp, #16
    mov x2, #1
    mov x8, #64
    svc #0
    ldp x29, x30, [sp], #32
    ret

.globl __dbg_hit
__dbg_hit:
    stp x29, x30, [sp, #-64]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    stp x21, x22, [sp, #32]
    mov x19, x0
    mov x20, x1
    mov x21, x2
    cbz x20, .Ldbg_hit_out
    mov x0, x20
    bl _F0.__gc_block_of
    cbz x0, .Ldbg_hit_out
    ldr w22, [x0]
    cbz w22, .Ldbg_hit_out
    movz x0, #0xAA
    bl __dbg_hex
    sub x0, x21, x19
    bl __dbg_hex
    mov x0, x19
    bl __dbg_hex
    mov x0, x20
    bl __dbg_hex
    mov x0, x22
    bl __dbg_hex
    bl __dbg_nl
.Ldbg_hit_out:
    ldp x19, x20, [sp, #16]
    ldp x21, x22, [sp, #32]
    ldp x29, x30, [sp], #64
    ret
"""

out = src[:start] + new_scan + src[end:] + "\n.section .text\n" + helpers
open(sys.argv[2], "w").write(out)
print("patched", len(src), "->", len(out))
