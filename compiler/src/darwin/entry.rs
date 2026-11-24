// SPDX-License-Identifier: MPL-2.0
pub const ENTRY_SYMBOL: &str = "__start";

pub const ENTRY_PROLOGUE_A64: &str = "\
__start:
    add x3, x0, #3
    lsl x3, x3, #3
    add x3, x3, #15
    and x3, x3, #-16
    sub sp, sp, x3
    str x0, [sp]
    mov x4, xzr
Lfmac_args_copy:
    cmp x4, x0
    b.hs Lfmac_args_done
    ldr x5, [x1, x4, lsl #3]
    add x6, x4, #1
    str x5, [sp, x6, lsl #3]
    add x4, x4, #1
    b Lfmac_args_copy
Lfmac_args_done:
    add x6, x4, #1
    str xzr, [sp, x6, lsl #3]
    add x6, x6, #1
    str xzr, [sp, x6, lsl #3]
";

pub const ENTRY_PROLOGUE_X86: &str = "\
__start:
    lea rax, [rdi+3]
    shl rax, 3
    add rax, 15
    and rax, -16
    sub rsp, rax
    mov qword ptr [rsp], rdi
    xor ecx, ecx
Lfmac_args_copy:
    cmp rcx, rdi
    jae Lfmac_args_done
    mov rdx, qword ptr [rsi+rcx*8]
    mov qword ptr [rsp+rcx*8+8], rdx
    inc rcx
    jmp Lfmac_args_copy
Lfmac_args_done:
    mov qword ptr [rsp+rcx*8+8], 0
    mov qword ptr [rsp+rcx*8+16], 0
";

pub fn prologue() -> &'static str {
    if crate::target::active() == crate::target::Target::Aarch64 {
        ENTRY_PROLOGUE_A64
    } else {
        ENTRY_PROLOGUE_X86
    }
}
