// SPDX-License-Identifier: MPL-2.0
use super::syscalls::{open_flags, Entry, Prep, Ret, A64_O_DIRECTORY, CLOCK_IDS, ENOSYS, MMAP_FLAGS, STUB_SYMBOL, TABLE};

pub fn stub_asm() -> String {
    let mut s = String::new();
    s.push_str(".text\n.p2align 2\n");
    s.push_str(&format!(".globl {}\n{}:\n", STUB_SYMBOL, STUB_SYMBOL));
    s.push_str("    stp x29, x30, [sp, #-16]!\n    mov x29, sp\n");
    for e in TABLE {
        s.push_str(&format!("    cmp x8, #{}\n    b.eq Lfmac_sys{}\n", e.a64, e.a64));
    }
    s.push_str(&format!("    mov x0, #-{}\n    b Lfmac_ret\n", ENOSYS));
    for e in TABLE {
        s.push_str(&entry_asm(e));
    }
    s.push_str("Lfmac_errno:\n    bl ___error\n    ldr w0, [x0]\n    neg x0, x0\n");
    s.push_str("Lfmac_ret:\n    ldp x29, x30, [sp], #16\n    ret\n");
    s
}

fn entry_asm(e: &Entry) -> String {
    let mut s = format!("Lfmac_sys{}:\n", e.a64);
    if e.ret == Ret::Zero {
        s.push_str("    mov x0, xzr\n    b Lfmac_ret\n");
        return s;
    }
    s.push_str(&prep_asm(e.prep));
    let variadic = e.prep == Prep::Open;
    if variadic {
        s.push_str("    sub sp, sp, #16\n    str x3, [sp]\n");
    }
    s.push_str(&format!("    bl {}\n", e.symbol));
    if variadic {
        s.push_str("    add sp, sp, #16\n");
    }
    if e.ret == Ret::Int {
        s.push_str("    sxtw x0, w0\n");
    }
    s.push_str("    cmn x0, #1\n    b.eq Lfmac_errno\n    b Lfmac_ret\n");
    s
}

fn prep_asm(prep: Prep) -> String {
    match prep {
        Prep::None => String::new(),
        Prep::Open => format!("    mov x0, x1\n{}    mov x1, x9\n", translate_bits("x2", "x9", &open_flags(A64_O_DIRECTORY), "and x9, x2, #3")),
        Prep::Mmap => translate_bits("x3", "x9", MMAP_FLAGS, "mov x10, #0x13\n    and x9, x3, x10") + "    mov x3, x9\n",
        Prep::ClockId => translate_ids("x0", CLOCK_IDS),
        Prep::UnlinkAt | Prep::MkdirAt => "    mov x0, x1\n    mov x1, x2\n".to_string(),
        Prep::Pipe | Prep::Dup => String::new(),
    }
}

fn translate_bits(src: &str, acc: &str, pairs: &[(u32, u32)], init: &str) -> String {
    let mut s = format!("    {}\n", init);
    for (linux, darwin) in pairs {
        s.push_str(&format!(
            "    mov x10, #{}\n    mov x11, #{}\n    tst {}, x10\n    csel x10, x11, xzr, ne\n    orr {}, {}, x10\n",
            linux, darwin, src, acc, acc
        ));
    }
    s
}

fn translate_ids(reg: &str, pairs: &[(u32, u32)]) -> String {
    let mut s = String::new();
    for (i, (linux, darwin)) in pairs.iter().enumerate() {
        s.push_str(&format!(
            "    cmp {}, #{}\n    b.ne Lfmac_clk{}\n    mov {}, #{}\n    b Lfmac_clkdone\nLfmac_clk{}:\n",
            reg, linux, i, reg, darwin, i
        ));
    }
    s.push_str("Lfmac_clkdone:\n");
    s
}
