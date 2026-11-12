    .section .text.boot, "ax"
    .global _boot
_boot:
    mrs x0, CurrentEL
    lsr x0, x0, #2
    cmp x0, #2
    b.ne 1f
    mov x0, #(1 << 31)
    msr hcr_el2, x0
    mov x0, #0x3c5
    msr spsr_el2, x0
    adr x0, 1f
    msr elr_el2, x0
    eret
1:
    ldr x0, =__stack_top
    mov sp, x0
    ldr x0, =__bss_start
    ldr x1, =__bss_end
2:
    cmp x0, x1
    b.hs 3f
    str xzr, [x0], #8
    b 2b
3:
    bl KERN_START
4:
    wfe
    b 4b

    .global osum_panic
osum_panic:
    wfe
    b osum_panic

    .macro vector_hold
    .balign 0x80
    wfe
    b .
    .endm

    .section .text.vec, "ax"
    .balign 0x800
    .global VECTORS
VECTORS:
    vector_hold
    vector_hold
    vector_hold
    vector_hold
    .balign 0x80
    b KERN_TRAP
    vector_hold
    vector_hold
    vector_hold
    vector_hold
    vector_hold
    vector_hold
    vector_hold
    vector_hold
    vector_hold
    vector_hold
    vector_hold
