// SPDX-License-Identifier: MPL-2.0
pub const ENTRY_SYMBOL: &str = "__start";

pub const ENTRY_PROLOGUE: &str = "\
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
