// SPDX-License-Identifier: MPL-2.0
pub const ENOSYS: i64 = 38;
pub const STUB_SYMBOL: &str = "__Fmac.syscall";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ret {
    Wide,
    Int,
    Zero,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Prep {
    None,
    Open,
    Mmap,
    ClockId,
    UnlinkAt,
    MkdirAt,
    Pipe,
    Dup,
}

pub struct Entry {
    pub a64: u32,
    pub x86: u32,
    pub symbol: &'static str,
    pub ret: Ret,
    pub prep: Prep,
}

pub const OPEN_FLAGS: &[(u32, u32)] = &[
    (0x40, 0x200),
    (0x80, 0x800),
    (0x200, 0x400),
    (0x400, 0x8),
    (0x800, 0x4),
    (0x80000, 0x0100_0000),
];

pub const DARWIN_O_DIRECTORY: u32 = 0x0010_0000;
pub const A64_O_DIRECTORY: u32 = 0x4000;
pub const X86_O_DIRECTORY: u32 = 0x10000;

pub fn open_flags(directory: u32) -> Vec<(u32, u32)> {
    let mut v = OPEN_FLAGS.to_vec();
    v.push((directory, DARWIN_O_DIRECTORY));
    v
}

pub const MMAP_FLAGS: &[(u32, u32)] = &[(0x20, 0x1000), (0x4000, 0x40)];

pub const CLOCK_IDS: &[(u32, u32)] = &[(1, 6), (2, 12), (3, 16), (5, 0), (6, 6)];

pub const TABLE: &[Entry] = &[
    Entry { a64: 64, x86: 1, symbol: "_write", ret: Ret::Wide, prep: Prep::None },
    Entry { a64: 63, x86: 0, symbol: "_read", ret: Ret::Wide, prep: Prep::None },
    Entry { a64: 57, x86: 3, symbol: "_close", ret: Ret::Int, prep: Prep::None },
    Entry { a64: 56, x86: 2, symbol: "_open", ret: Ret::Int, prep: Prep::Open },
    Entry { a64: 62, x86: 8, symbol: "_lseek", ret: Ret::Wide, prep: Prep::None },
    Entry { a64: 93, x86: 60, symbol: "__exit", ret: Ret::Int, prep: Prep::None },
    Entry { a64: 94, x86: 231, symbol: "__exit", ret: Ret::Int, prep: Prep::None },
    Entry { a64: 222, x86: 9, symbol: "_mmap", ret: Ret::Wide, prep: Prep::Mmap },
    Entry { a64: 215, x86: 11, symbol: "_munmap", ret: Ret::Int, prep: Prep::None },
    Entry { a64: 226, x86: 10, symbol: "_mprotect", ret: Ret::Int, prep: Prep::None },
    Entry { a64: 233, x86: 28, symbol: "", ret: Ret::Zero, prep: Prep::None },
    Entry { a64: 132, x86: 131, symbol: "", ret: Ret::Zero, prep: Prep::None },
    Entry { a64: 134, x86: 13, symbol: "", ret: Ret::Zero, prep: Prep::None },
    Entry { a64: 135, x86: 14, symbol: "", ret: Ret::Zero, prep: Prep::None },
    Entry { a64: 113, x86: 228, symbol: "_clock_gettime", ret: Ret::Int, prep: Prep::ClockId },
    Entry { a64: 101, x86: 35, symbol: "_nanosleep", ret: Ret::Int, prep: Prep::None },
    Entry { a64: 172, x86: 39, symbol: "_getpid", ret: Ret::Int, prep: Prep::None },
    Entry { a64: 124, x86: 24, symbol: "_sched_yield", ret: Ret::Int, prep: Prep::None },
    Entry { a64: 82, x86: 74, symbol: "_fsync", ret: Ret::Int, prep: Prep::None },
    Entry { a64: 46, x86: 77, symbol: "_ftruncate", ret: Ret::Int, prep: Prep::None },
    Entry { a64: 35, x86: 87, symbol: "_unlink", ret: Ret::Int, prep: Prep::UnlinkAt },
    Entry { a64: 34, x86: 83, symbol: "_mkdir", ret: Ret::Int, prep: Prep::MkdirAt },
    Entry { a64: 59, x86: 22, symbol: "_pipe", ret: Ret::Int, prep: Prep::Pipe },
    Entry { a64: 24, x86: 33, symbol: "_dup2", ret: Ret::Int, prep: Prep::Dup },
    Entry { a64: 129, x86: 62, symbol: "_kill", ret: Ret::Int, prep: Prep::None },
];

pub fn imported_symbols() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = TABLE.iter().map(|e| e.symbol).filter(|s| !s.is_empty()).collect();
    v.push("___error");
    v.push("dyld_stub_binder");
    v.sort_unstable();
    v.dedup();
    v
}
