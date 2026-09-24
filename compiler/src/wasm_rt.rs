// SPDX-License-Identifier: MPL-2.0
//! **ROUND WASM — the runtime of a Firn program in the browser.**
//!
//! Nine small functions, written here instruction by instruction — the
//! WebAssembly counterpart of the hand written trampoline in `panic_rt.rs`.
//! They are what the Linux kernel and the `_start` block are for a native
//! Firn program:
//!
//! | function | what it replaces |
//! |---|---|
//! | `_start` | the `_start` block of `codegen_x86.rs`: collector setup, `main(start)`, `exit` |
//! | `__firn_mmap` / `__firn_munmap` | `mmap(2)`/`munmap(2)` of anonymous memory — on top of `memory.grow` |
//! | `__firn_clock_gettime` | `clock_gettime(2)`, on the host's nanosecond clock |
//! | `__firn_nanosleep` | `nanosleep(2)`, on the host's sleep |
//! | `__firn_futex` | `futex(2)` with the semantics of a process with ONE thread |
//! | `__firn_panic` + `__firn_dec` | the panic trampoline of `panic_rt.rs`: the same two writes, the same text, exit code 101 |
//! | `__firn_stack_overflow` | the `SIGSEGV` of a native stack overflow |
//!
//! ## The memory allocator, and why it is this simple
//!
//! WebAssembly memory can grow (`memory.grow`) but never shrink. `mmap`
//! therefore hands out page runs (4096 octets, the unit `lib/std/rt.fi` and
//! the collector count in) from a region that only grows, and `munmap`
//! does not return memory to the engine: it puts the run into a free list,
//! kept sorted by address and merged with its neighbours, and the next
//! `mmap` takes the first run that is large enough (first fit). The free
//! list lives IN the free memory itself — eight octets at the start of each
//! run: the next run, the size. A run that is handed out again is zeroed
//! first, because `mmap(2)` promises zeroed pages and the collector relies
//! on it.
//!
//! That is the whole allocator. It is not fast, and it does not have to be:
//! both callers (`rt.heap_alloc`, the collector's chunks of 256 KiB) ask
//! for large pieces rarely and manage the small ones themselves.

use crate::wasm_enc::{self as w, Ins, VT};

pub struct Env {
    pub start: u32,
    pub mmap: u32,
    pub munmap: u32,
    pub clock: u32,
    pub nanosleep: u32,
    pub futex: u32,
    pub panic: u32,
    pub dec: u32,
    pub overflow: u32,
    pub h_write: u32,
    pub h_exit: u32,
    pub h_clock: u32,
    pub h_sleep: u32,
    pub g_heap_top: u32,
    pub g_free: u32,
    pub heap_base: u32,
    pub argv: u32,
    pub buf: u32,
    pub lit_a: u32,
    pub lit_b: u32,
    pub lit_idx: u32,
    pub lit_len: u32,
    pub lit_close: u32,
    pub msg_overflow: (u32, u32),
    pub msg_futex: (u32, u32),
    pub gc_init: Option<u32>,
    pub main: u32,
    pub main_takes_start: bool,
    pub main_ret: Option<VT>,
    pub handler: Option<u32>,
    pub handler_ret: Option<VT>,
}

/// Exit code of a native process killed by `SIGSEGV`, as the shell reports
/// it -- a stack overflow ends the same way here.
const EXIT_SEGV: i32 = 139;
/// Exit code of the checked arithmetic (`panic_rt.rs`).
const EXIT_PANIC: i32 = 101;
/// Exit code of a failure of the runtime itself (as `__gc_abort`).
const EXIT_RUNTIME: i32 = 70;

fn push(wm: &mut w::Module, want: u32, params: &[VT], results: &[VT], locals: &[VT], body: Vec<Ins>, sym: &str) -> Result<(), String> {
    let at = wm.func_index(wm.funcs.len());
    if at != want {
        return Err(format!("internal error: wasm32 runtime '{}' at index {} instead of {}", sym, at, want));
    }
    let ty = wm.type_index(w::FuncType { params: params.to_vec(), results: results.to_vec() });
    wm.funcs.push(w::Func { ty, locals: locals.to_vec(), body, sym: sym.to_string() });
    Ok(())
}

pub fn build(wm: &mut w::Module, e: &Env) -> Result<(), String> {
    start(wm, e)?;
    mmap(wm, e)?;
    munmap(wm, e)?;
    clock(wm, e)?;
    nanosleep(wm, e)?;
    futex(wm, e)?;
    panic(wm, e)?;
    dec(wm, e)?;
    overflow(wm, e)
}

use w::{
    I32_ADD, I32_AND, I32_EQ, I32_EQZ, I32_GE_U, I32_GT_U, I32_LOAD, I32_LOAD8_U, I32_LT_U, I32_NE, I32_OR, I32_STORE,
    I32_STORE8, I32_SUB, I32_WRAP_I64, I64_ADD, I64_AND, I64_DIV_U, I64_EQ, I64_EQZ, I64_EXTEND_I32_S,
    I64_EXTEND_I32_U, I64_GT_U, I64_LOAD, I64_LT_S, I64_MUL, I64_NE, I64_REM_U, I64_SHL, I64_SHR_U, I64_STORE,
    I64_SUB,
};

fn n(x: w::Num) -> Ins {
    Ins::Num(x)
}

/// `_start`: exactly the order of the native `_start` block -- the
/// collector first (ROUND 88: "the collector starts itself"), then `main`
/// with the address of the argument block, then `exit` with its result.
fn start(wm: &mut w::Module, e: &Env) -> Result<(), String> {
    use Ins::*;
    let mut b = Vec::new();
    if let Some(gi) = e.gc_init {
        b.push(Call(gi));
        b.push(Drop);
    }
    if e.main_takes_start {
        b.push(I64Const(e.argv as i64));
    }
    b.push(Call(e.main));
    match e.main_ret {
        Some(VT::I32) => {}
        Some(VT::I64) => b.push(n(I32_WRAP_I64)),
        Some(_) => {
            b.push(Drop);
            b.push(I32Const(0));
        }
        None => b.push(I32Const(0)),
    }
    b.push(Call(e.h_exit));
    b.push(Unreachable);
    push(wm, e.start, &[], &[], &[], b, "_start")
}

/// `mmap(addr, len, prot, flags, fd, off)` -> address or -errno.
///
/// Locals: 6 n, 7 prev, 8 cur, 9 size, 10 rest, 11 p (all i32), 12 need (i64).
fn mmap(wm: &mut w::Module, e: &Env) -> Result<(), String> {
    use Ins::*;
    let mut b: Vec<Ins> = Vec::new();
    // Anonymous memory only (MAP_ANONYMOUS = 0x20): a file mapping has no
    // counterpart, and the compiler already refused a constant one.
    b.extend([LocalGet(3), I64Const(0x20), n(I64_AND), n(I64_EQZ), If, I64Const(-19), Return, End]);
    // No fixed address (MAP_FIXED = 0x10).
    b.extend([LocalGet(3), I64Const(0x10), n(I64_AND), n(I64_EQZ), n(I32_EQZ), If, I64Const(-22), Return, End]);
    b.extend([LocalGet(1), n(I64_EQZ), If, I64Const(-22), Return, End]);
    b.extend([LocalGet(1), I64Const(0xFFFF_0000), n(I64_GT_U), If, I64Const(-12), Return, End]);
    // n = len rounded up to whole pages of 4096
    b.extend([LocalGet(1), I64Const(4095), n(I64_ADD), I64Const(-4096), n(I64_AND), n(I32_WRAP_I64), LocalSet(6)]);
    // first fit through the free list
    b.extend([I32Const(0), LocalSet(7), GlobalGet(e.g_free), LocalSet(8)]);
    b.extend([Block, Loop]);
    b.extend([LocalGet(8), n(I32_EQZ), BrIf(1)]);
    b.extend([LocalGet(8), Load(I32_LOAD, 4), LocalSet(9)]);
    b.extend([LocalGet(9), LocalGet(6), n(I32_GE_U), If]);
    //   exactly the size: unlink; larger: the tail stays in the list
    b.extend([LocalGet(9), LocalGet(6), n(I32_EQ), If]);
    b.extend([LocalGet(8), Load(I32_LOAD, 0), LocalSet(10)]);
    b.push(Else);
    b.extend([LocalGet(8), LocalGet(6), n(I32_ADD), LocalSet(10)]);
    b.extend([LocalGet(10), LocalGet(8), Load(I32_LOAD, 0), Store(I32_STORE, 0)]);
    b.extend([LocalGet(10), LocalGet(9), LocalGet(6), n(I32_SUB), Store(I32_STORE, 4)]);
    b.push(End);
    b.extend([LocalGet(7), n(I32_EQZ), If, LocalGet(10), GlobalSet(e.g_free), Else, LocalGet(7), LocalGet(10), Store(I32_STORE, 0), End]);
    //   zeroed, as mmap(2) promises
    b.extend([LocalGet(8), I32Const(0), LocalGet(6), MemoryFill, LocalGet(8), n(I64_EXTEND_I32_U), Return]);
    b.push(End);
    b.extend([LocalGet(8), LocalSet(7), LocalGet(8), Load(I32_LOAD, 0), LocalSet(8), Br(0)]);
    b.extend([End, End]);
    // Nothing free fits: from the top of the heap, growing the memory.
    b.extend([GlobalGet(e.g_heap_top), LocalSet(11)]);
    b.extend([LocalGet(11), n(I64_EXTEND_I32_U), LocalGet(6), n(I64_EXTEND_I32_U), n(I64_ADD), LocalTee(12)]);
    b.extend([I64Const(0xFFFF_0000), n(I64_GT_U), If, I64Const(-12), Return, End]);
    b.extend([LocalGet(12), MemorySize, n(I64_EXTEND_I32_U), I64Const(16), n(I64_SHL), n(I64_GT_U), If]);
    b.extend([
        LocalGet(12),
        MemorySize,
        n(I64_EXTEND_I32_U),
        I64Const(16),
        n(I64_SHL),
        n(I64_SUB),
        I64Const(65535),
        n(I64_ADD),
        I64Const(16),
        n(I64_SHR_U),
        n(I32_WRAP_I64),
        MemoryGrow,
        I32Const(-1),
        n(I32_EQ),
        If,
        I64Const(-12),
        Return,
        End,
    ]);
    b.push(End);
    b.extend([LocalGet(12), n(I32_WRAP_I64), GlobalSet(e.g_heap_top), LocalGet(11), n(I64_EXTEND_I32_U)]);
    let l = [VT::I32, VT::I32, VT::I32, VT::I32, VT::I32, VT::I32, VT::I64];
    push(wm, e.mmap, &[VT::I64; 6], &[VT::I64], &l, b, "__firn_mmap")
}

/// `munmap(addr, len)` -> 0 or -errno. The run goes into the free list,
/// merged with the neighbours it touches.
///
/// Locals: 2 a, 3 n, 4 prev, 5 cur (all i32).
fn munmap(wm: &mut w::Module, e: &Env) -> Result<(), String> {
    use Ins::*;
    let mut b: Vec<Ins> = Vec::new();
    b.extend([LocalGet(0), I64Const(4095), n(I64_AND), I64Const(0), n(I64_NE), If, I64Const(-22), Return, End]);
    b.extend([LocalGet(1), n(I64_EQZ), If, I64Const(-22), Return, End]);
    b.extend([LocalGet(0), n(I32_WRAP_I64), LocalSet(2)]);
    b.extend([LocalGet(1), I64Const(4095), n(I64_ADD), I64Const(-4096), n(I64_AND), n(I32_WRAP_I64), LocalSet(3)]);
    // Only what mmap handed out may come back: a range outside the heap
    // would put the data or the stack into the free list.
    b.extend([
        LocalGet(2),
        I32Const(e.heap_base as i32),
        n(I32_LT_U),
        LocalGet(2),
        LocalGet(3),
        n(I32_ADD),
        GlobalGet(e.g_heap_top),
        n(I32_GT_U),
        n(I32_OR),
        If,
        I64Const(-22),
        Return,
        End,
    ]);
    b.extend([I32Const(0), LocalSet(4), GlobalGet(e.g_free), LocalSet(5)]);
    b.extend([Block, Loop]);
    b.extend([LocalGet(5), n(I32_EQZ), BrIf(1)]);
    b.extend([LocalGet(5), LocalGet(2), n(I32_GT_U), BrIf(1)]);
    b.extend([LocalGet(5), LocalSet(4), LocalGet(5), Load(I32_LOAD, 0), LocalSet(5), Br(0)]);
    b.extend([End, End]);
    b.extend([LocalGet(2), LocalGet(5), Store(I32_STORE, 0)]);
    b.extend([LocalGet(2), LocalGet(3), Store(I32_STORE, 4)]);
    // merge with the next run
    b.extend([LocalGet(5), If]);
    b.extend([LocalGet(2), LocalGet(3), n(I32_ADD), LocalGet(5), n(I32_EQ), If]);
    b.extend([LocalGet(2), LocalGet(5), Load(I32_LOAD, 0), Store(I32_STORE, 0)]);
    b.extend([LocalGet(2), LocalGet(3), LocalGet(5), Load(I32_LOAD, 4), n(I32_ADD), Store(I32_STORE, 4)]);
    b.extend([End, End]);
    // link behind the previous run, merging with it when it touches
    b.extend([LocalGet(4), n(I32_EQZ), If, LocalGet(2), GlobalSet(e.g_free), Else]);
    b.extend([LocalGet(4), LocalGet(4), Load(I32_LOAD, 4), n(I32_ADD), LocalGet(2), n(I32_EQ), If]);
    b.extend([LocalGet(4), LocalGet(4), Load(I32_LOAD, 4), LocalGet(2), Load(I32_LOAD, 4), n(I32_ADD), Store(I32_STORE, 4)]);
    b.extend([LocalGet(4), LocalGet(2), Load(I32_LOAD, 0), Store(I32_STORE, 0)]);
    b.push(Else);
    b.extend([LocalGet(4), LocalGet(2), Store(I32_STORE, 0)]);
    b.extend([End, End]);
    b.push(I64Const(0));
    push(wm, e.munmap, &[VT::I64, VT::I64], &[VT::I64], &[VT::I32; 4], b, "__firn_munmap")
}

/// `clock_gettime(clock, ts)`: the host's nanoseconds split into the
/// `timespec` `{ tv_sec, tv_nsec }`.
fn clock(wm: &mut w::Module, e: &Env) -> Result<(), String> {
    use Ins::*;
    let b = vec![
        LocalGet(0),
        n(I32_WRAP_I64),
        Call(e.h_clock),
        LocalSet(2),
        LocalGet(1),
        n(I32_WRAP_I64),
        LocalGet(2),
        I64Const(1_000_000_000),
        n(I64_DIV_U),
        Store(I64_STORE, 0),
        LocalGet(1),
        n(I32_WRAP_I64),
        LocalGet(2),
        I64Const(1_000_000_000),
        n(I64_REM_U),
        Store(I64_STORE, 8),
        I64Const(0),
    ];
    push(wm, e.clock, &[VT::I64, VT::I64], &[VT::I64], &[VT::I64], b, "__firn_clock_gettime")
}

/// `nanosleep(req, rem)`: the `timespec` in nanoseconds, to the host.
fn nanosleep(wm: &mut w::Module, e: &Env) -> Result<(), String> {
    use Ins::*;
    let b = vec![
        LocalGet(0),
        n(I32_WRAP_I64),
        Load(I64_LOAD, 0),
        I64Const(1_000_000_000),
        n(I64_MUL),
        LocalGet(0),
        n(I32_WRAP_I64),
        Load(I64_LOAD, 8),
        n(I64_ADD),
        Call(e.h_sleep),
        n(I64_EXTEND_I32_S),
    ];
    push(wm, e.nanosleep, &[VT::I64, VT::I64], &[VT::I64], &[], b, "__firn_nanosleep")
}

/// `futex(addr, op, val)` in a process with exactly one thread. WAKE wakes
/// nobody (there is nobody), WAIT on a changed word returns `EAGAIN` as the
/// kernel would -- and WAIT on an unchanged word would block forever, since
/// no other thread exists to change it. Natively that is a hang; here it is
/// said, and the program ends.
fn futex(wm: &mut w::Module, e: &Env) -> Result<(), String> {
    use Ins::*;
    let mut b = vec![
        LocalGet(1),
        I64Const(127),
        n(I64_AND),
        I64Const(1),
        n(I64_EQ),
        If,
        I64Const(0),
        Return,
        End,
    ];
    b.extend([LocalGet(1), I64Const(127), n(I64_AND), n(I64_EQZ), If]);
    b.extend([LocalGet(0), n(I32_WRAP_I64), Load(I32_LOAD, 0), LocalGet(2), n(I32_WRAP_I64), n(I32_NE), If, I64Const(-11), Return, End]);
    b.extend([
        I32Const(2),
        I32Const(e.msg_futex.0 as i32),
        I32Const(e.msg_futex.1 as i32),
        Call(e.h_write),
        Drop,
        I32Const(EXIT_RUNTIME),
        Call(e.h_exit),
        Unreachable,
    ]);
    b.push(End);
    b.push(I64Const(-38));
    push(wm, e.futex, &[VT::I64, VT::I64, VT::I64], &[VT::I64], &[], b, "__firn_futex")
}

/// The panic of the checked arithmetic -- `panic_rt.rs::trampoline_asm`,
/// the same two writes to file descriptor 2:
///
/// ```text
/// <message>               (first write)
///  (a=<N> b=<M>)\n        (second write; " (index=<N> len=<M>)" for an index)
/// ```
///
/// then exit code 101. With a `#[panic_handler]` the handler gets the five
/// values of `panic_rt::HANDLER_SIG` instead, and exit code 101 follows if
/// it comes back.
///
/// Params: 0 entry (1 = index form), 1 msg, 2 len, 3 a, 4 b, 5 code,
/// 6 unsigned. Local 7: the write position.
fn panic(wm: &mut w::Module, e: &Env) -> Result<(), String> {
    use Ins::*;
    let mut b = Vec::new();
    if let Some(h) = e.handler {
        b.extend([LocalGet(1), n(I64_EXTEND_I32_U), LocalGet(2), n(I64_EXTEND_I32_U), LocalGet(3), LocalGet(4), LocalGet(5), Call(h)]);
        if e.handler_ret.is_some() {
            b.push(Drop);
        }
        b.extend([I32Const(EXIT_PANIC), Call(e.h_exit), Unreachable]);
    } else {
        let lit = |b: &mut Vec<Ins>, at: u32, len: i32| {
            b.extend([LocalGet(7), I32Const(at as i32), I32Const(len), MemoryCopy, LocalGet(7), I32Const(len), n(I32_ADD), LocalSet(7)]);
        };
        b.extend([I32Const(2), LocalGet(1), LocalGet(2), Call(e.h_write), Drop]);
        b.extend([I32Const(e.buf as i32), LocalSet(7)]);
        b.extend([LocalGet(0), If]);
        lit(&mut b, e.lit_idx, 8);
        b.push(Else);
        lit(&mut b, e.lit_a, 4);
        b.push(End);
        b.extend([LocalGet(7), LocalGet(3), LocalGet(6), Call(e.dec), LocalSet(7)]);
        b.extend([LocalGet(0), If]);
        lit(&mut b, e.lit_len, 5);
        b.push(Else);
        lit(&mut b, e.lit_b, 3);
        b.push(End);
        b.extend([LocalGet(7), LocalGet(4), LocalGet(6), Call(e.dec), LocalSet(7)]);
        lit(&mut b, e.lit_close, 2);
        b.extend([I32Const(2), I32Const(e.buf as i32), LocalGet(7), I32Const(e.buf as i32), n(I32_SUB), Call(e.h_write), Drop]);
        b.extend([I32Const(EXIT_PANIC), Call(e.h_exit), Unreachable]);
    }
    push(
        wm,
        e.panic,
        &[VT::I32, VT::I32, VT::I32, VT::I64, VT::I64, VT::I64, VT::I32],
        &[],
        &[VT::I32],
        b,
        "__firn_panic",
    )
}

/// The decimal text of `v` at `pos` -> the position behind it. Signed
/// unless `unsigned` is set; `-MIN` stays MIN as a bit pattern and is then
/// read unsigned -- `9223372036854775808`, the same trick as
/// `.Lpanic_i64_dec`.
///
/// Params: 0 pos, 1 v, 2 unsigned. Locals: 3 lo, 4 hi, 5 t.
fn dec(wm: &mut w::Module, e: &Env) -> Result<(), String> {
    use Ins::*;
    let mut b = Vec::new();
    b.extend([LocalGet(2), n(I32_EQZ), If]);
    b.extend([LocalGet(1), I64Const(0), n(I64_LT_S), If]);
    b.extend([LocalGet(0), I32Const(45), Store(I32_STORE8, 0)]);
    b.extend([LocalGet(0), I32Const(1), n(I32_ADD), LocalSet(0)]);
    b.extend([I64Const(0), LocalGet(1), n(I64_SUB), LocalSet(1)]);
    b.extend([End, End]);
    b.extend([LocalGet(1), n(I64_EQZ), If]);
    b.extend([LocalGet(0), I32Const(48), Store(I32_STORE8, 0)]);
    b.extend([LocalGet(0), I32Const(1), n(I32_ADD), Return]);
    b.push(End);
    b.extend([LocalGet(0), LocalSet(3)]);
    b.push(Loop);
    b.extend([
        LocalGet(0),
        LocalGet(1),
        I64Const(10),
        n(I64_REM_U),
        n(I32_WRAP_I64),
        I32Const(48),
        n(I32_ADD),
        Store(I32_STORE8, 0),
    ]);
    b.extend([LocalGet(0), I32Const(1), n(I32_ADD), LocalSet(0)]);
    b.extend([LocalGet(1), I64Const(10), n(I64_DIV_U), LocalTee(1), n(I64_EQZ), n(I32_EQZ), BrIf(0)]);
    b.push(End);
    // the digits came out backwards: reverse [lo, hi]
    b.extend([LocalGet(0), I32Const(1), n(I32_SUB), LocalSet(4)]);
    b.extend([Block, Loop]);
    b.extend([LocalGet(3), LocalGet(4), n(I32_GE_U), BrIf(1)]);
    b.extend([LocalGet(3), Load(I32_LOAD8_U, 0), LocalSet(5)]);
    b.extend([LocalGet(3), LocalGet(4), Load(I32_LOAD8_U, 0), Store(I32_STORE8, 0)]);
    b.extend([LocalGet(4), LocalGet(5), Store(I32_STORE8, 0)]);
    b.extend([LocalGet(3), I32Const(1), n(I32_ADD), LocalSet(3)]);
    b.extend([LocalGet(4), I32Const(1), n(I32_SUB), LocalSet(4)]);
    b.push(Br(0));
    b.extend([End, End]);
    b.push(LocalGet(0));
    push(wm, e.dec, &[VT::I32, VT::I64, VT::I32], &[VT::I32], &[VT::I32; 3], b, "__firn_dec")
}

/// The end of a program whose shadow stack ran out.
fn overflow(wm: &mut w::Module, e: &Env) -> Result<(), String> {
    use Ins::*;
    let b = vec![
        I32Const(2),
        I32Const(e.msg_overflow.0 as i32),
        I32Const(e.msg_overflow.1 as i32),
        Call(e.h_write),
        Drop,
        I32Const(EXIT_SEGV),
        Call(e.h_exit),
        Unreachable,
    ];
    let _ = (I32_AND, I32_LOAD8_U);
    push(wm, e.overflow, &[], &[], &[], b, "__firn_stack_overflow")
}
