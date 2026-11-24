// SPDX-License-Identifier: MPL-2.0
const ENOSYS: i64 = 38;
pub const STUB_SYMBOL: &str = "__Fmac.syscall";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ret {
    Wide,
    Int,
    Zero,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prep {
    None,
    Open,
    Mmap,
    ClockId,
    UnlinkAt,
    MkdirAt,
    Pipe,
    Dup,
}

struct Entry {
    number: u32,
    symbol: &'static str,
    ret: Ret,
    prep: Prep,
}

const OPEN_FLAGS: &[(u32, u32)] = &[
    (0x40, 0x200),
    (0x80, 0x800),
    (0x200, 0x400),
    (0x400, 0x8),
    (0x800, 0x4),
    (0x80000, 0x0100_0000),
    (0x4000, 0x0010_0000),
];

const MMAP_FLAGS: &[(u32, u32)] = &[(0x20, 0x1000), (0x4000, 0x40)];

const CLOCK_IDS: &[(u32, u32)] = &[(1, 6), (2, 12), (3, 16), (5, 0), (6, 6)];

const TABLE: &[Entry] = &[
    Entry { number: 64, symbol: "_write", ret: Ret::Wide, prep: Prep::None },
    Entry { number: 63, symbol: "_read", ret: Ret::Wide, prep: Prep::None },
    Entry { number: 57, symbol: "_close", ret: Ret::Int, prep: Prep::None },
    Entry { number: 56, symbol: "_open", ret: Ret::Int, prep: Prep::Open },
    Entry { number: 62, symbol: "_lseek", ret: Ret::Wide, prep: Prep::None },
    Entry { number: 93, symbol: "__exit", ret: Ret::Int, prep: Prep::None },
    Entry { number: 94, symbol: "__exit", ret: Ret::Int, prep: Prep::None },
    Entry { number: 222, symbol: "_mmap", ret: Ret::Wide, prep: Prep::Mmap },
    Entry { number: 215, symbol: "_munmap", ret: Ret::Int, prep: Prep::None },
    Entry { number: 226, symbol: "_mprotect", ret: Ret::Int, prep: Prep::None },
    Entry { number: 233, symbol: "", ret: Ret::Zero, prep: Prep::None },
    Entry { number: 113, symbol: "_clock_gettime", ret: Ret::Int, prep: Prep::ClockId },
    Entry { number: 101, symbol: "_nanosleep", ret: Ret::Int, prep: Prep::None },
    Entry { number: 172, symbol: "_getpid", ret: Ret::Int, prep: Prep::None },
    Entry { number: 124, symbol: "_sched_yield", ret: Ret::Int, prep: Prep::None },
    Entry { number: 82, symbol: "_fsync", ret: Ret::Int, prep: Prep::None },
    Entry { number: 46, symbol: "_ftruncate", ret: Ret::Int, prep: Prep::None },
    Entry { number: 35, symbol: "_unlink", ret: Ret::Int, prep: Prep::UnlinkAt },
    Entry { number: 34, symbol: "_mkdir", ret: Ret::Int, prep: Prep::MkdirAt },
    Entry { number: 59, symbol: "_pipe", ret: Ret::Int, prep: Prep::Pipe },
    Entry { number: 24, symbol: "_dup2", ret: Ret::Int, prep: Prep::Dup },
    Entry { number: 129, symbol: "_kill", ret: Ret::Int, prep: Prep::None },
];

pub fn imported_symbols() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = TABLE.iter().map(|e| e.symbol).filter(|s| !s.is_empty()).collect();
    v.push("___error");
    v.push("dyld_stub_binder");
    v.sort_unstable();
    v.dedup();
    v
}

pub fn stub_asm() -> String {
    let mut s = String::new();
    s.push_str(".text\n.p2align 2\n");
    s.push_str(&format!(".globl {}\n{}:\n", STUB_SYMBOL, STUB_SYMBOL));
    s.push_str("    stp x29, x30, [sp, #-16]!\n    mov x29, sp\n");
    for e in TABLE {
        s.push_str(&format!("    cmp x8, #{}\n    b.eq Lfmac_sys{}\n", e.number, e.number));
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
    let mut s = format!("Lfmac_sys{}:\n", e.number);
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
        Prep::Open => format!("    mov x0, x1\n{}    mov x1, x9\n", translate_bits("x2", "x9", OPEN_FLAGS, "and x9, x2, #3")),
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
