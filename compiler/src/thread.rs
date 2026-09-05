// SPDX-License-Identifier: GPL-2.0-only
//! **Round 49 — threads.** The three primitives that make concurrency
//! possible at all. Everything else (stack, join, mutex, channel, the thread
//! safety of the collector) stands as readable Firn at `lib/gc/gc.fi`.
//!
//! ```firn
//! __thread_start(arg: u64, stack: u64, ctid: *mut u8) -> i64
//! __thread_self() -> *mut u8
//! __atomic_swap(p: *mut u64, expected: u64, new: u64) -> u64
//! ```
//!
//! ## Why `clone(2)` and not pthreads
//!
//! A Firn program is **freestanding**: its own `_start`, no libc, no dynamic
//! linker, every system service through `syscall` (SPEC §11).
//! `pthread_create` would have dragged the whole glibc along —
//! initialization, TLS model, signal handling, `__libc_start_main` — and
//! thereby given up exactly what makes the language. `clone(2)` is a system
//! call like `mmap`; it costs nothing beyond this file.
//!
//! ## Why that needs a primitive of its own and `syscall(56, …)` does not do
//!
//! That is the core: `clone` returns **twice** — in the creator with the
//! thread id, in the child with 0. The child returns with a **new `rsp`**,
//! though, while `rbp` and every callee-saved register are copies of the
//! creator. The code produced by the code generator addresses its frame
//! slots through `[rbp-off]` — so the child would write into the frame of
//! the CREATOR, on a stack that is not its own. There is no wording in the
//! language that avoids that; the transition has to happen in the same
//! instruction sequence as the system call. Exactly that is
//! `Op::ThreadSpawn`: system call, branch by return value, in the child
//! fetch the argument from the new stack, call the entry, then `exit(2)` —
//! **not** `exit_group(2)`, else an ending thread takes the whole process.
//!
//! ## Why a TLS self pointer
//!
//! At every allocation site the collector has to know which thread is
//! allocating right now (own free list, own grey buffer, own stack region).
//! `gettid(2)` would be a system call per allocation. The kernel can hand
//! the thread an `fs` base register instead (`arch_prctl(ARCH_SET_FS)`); the
//! block header of the thread carries its own address at offset 0, and
//! `mov rax, fs:0` is **one** instruction without a memory access outside
//! the cache line of the thread.
//!
//! ## Why a compare-and-swap joins them
//!
//! `docs/ROUND47.md` §3.2 names the gap: with `lock xadd` alone
//! neither a lock can be built (the transition "free -> taken" has to be
//! conditional) nor the atomic upgrade (weak -> strong) be closed.
//! `lock cmpxchg` is the smallest addition that settles both.

use crate::ast::Expr;
use crate::diag::Span;
use crate::fir::{FTy, Op, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

/// Create a thread.
pub(crate) const START: &str = "__thread_start";
/// Own thread block (TLS, `fs:0`).
pub(crate) const SELF: &str = "__thread_self";
/// Atomic compare-and-swap.
pub(crate) const CAS: &str = "__atomic_swap";

/// Name of the entry function that the child calls. It sits in the
/// collector runtime (`lib/gc/gc.fi`) and gets the thread block as its only
/// argument. A function pointer would be the alternative; stage 0 has none
/// (the same decision as with the dispatcher of the finalizers, round 47).
pub(crate) const ENTRY: &str = "__thread_entry";

/// `clone(2)` flags: shared address space, shared files, a real thread of
/// the same thread group, and the two TID flags out of which `thread_wait`
/// is built (`CLONE_CHILD_CLEARTID` makes the kernel zero the word at thread
/// end and wake up on it — exactly the mechanism of `pthread_join`).
pub(crate) const CLONE_FLAGS: u64 = 0x0000_0100  // CLONE_VM
    | 0x0000_0200                                 // CLONE_FS
    | 0x0000_0400                                 // CLONE_FILES
    | 0x0000_0800                                 // CLONE_SIGHAND
    | 0x0001_0000                                 // CLONE_THREAD
    | 0x0004_0000                                 // CLONE_SYSVSEM
    | 0x0010_0000                                 // CLONE_PARENT_SETTID
    | 0x0020_0000; // CLONE_CHILD_CLEARTID

/// Is this spelling one of the three primitives?
pub(crate) fn is_thread_call(name: &str) -> bool {
    name == START || name == SELF || name == CAS
}

// ----------------------------------------------------------------- Type phase

/// Hook from `sema::call`. `None` if this is none of the primitives or if
/// the program contains a function of the same spelling — that one wins then.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if !is_thread_call(name) || ck.fns.contains_key(name) {
        return None;
    }
    let _ = nspan;
    match name {
        SELF => {
            if !args.is_empty() {
                for a in args {
                    ck.type_out_expr(a);
                }
                ck.dg.error_note(
                    espan,
                    format!("'{}' expects no arguments, found {}", SELF, args.len()),
                    "the form is __thread_self() -> *mut u8",
                );
                return Some(Type::Error);
            }
            Some(Type::ptr(Type::U8, true))
        }
        START => {
            if args.len() != 3 {
                for a in args {
                    ck.type_out_expr(a);
                }
                ck.dg.error_note(
                    espan,
                    format!(
                        "'{}' expects exactly three arguments (argument, stack, tid word), found {}",
                        START,
                        args.len()
                    ),
                    "the form is __thread_start(arg: u64, stack: u64, ctid: *mut u8) -> i64",
                );
                return Some(Type::Error);
            }
            let at = ck.expr(&args[0], Some(&Type::U64));
            let st = ck.expr(&args[1], Some(&Type::U64));
            let ct = ck.expr(&args[2], Some(&Type::ptr(Type::U8, true)));
            if !at.is_error() && !fits_as_u64(&at) {
                ck.dg.error(
                    args[0].span,
                    format!("'{}' expects a u64 as argument, found {}", START, ck.tcx.name_of(&at)),
                );
                return Some(Type::Error);
            }
            if !st.is_error() && !fits_as_u64(&st) {
                ck.dg.error_note(
                    args[1].span,
                    format!(
                        "'{}' expects a u64 as stack, found {}",
                        START,
                        ck.tcx.name_of(&st)
                    ),
                    "the stack is the UPPER address of the thread stack (it grows downwards)",
                );
                return Some(Type::Error);
            }
            if !ct.is_error() && !is_ptr(&ct) {
                ck.dg.error_note(
                    args[2].span,
                    format!(
                        "'{}' expects a pointer to the tid word as third argument, found {}",
                        START,
                        ck.tcx.name_of(&ct)
                    ),
                    "the kernel writes the thread id there and zeroes it when the thread ends",
                );
                return Some(Type::Error);
            }
            Some(Type::I64)
        }
        _ => {
            // CAS
            if args.len() != 3 {
                for a in args {
                    ck.type_out_expr(a);
                }
                ck.dg.error_note(
                    espan,
                    format!(
                        "'{}' expects exactly three arguments (pointer, expected, new), found {}",
                        CAS,
                        args.len()
                    ),
                    "the form is __atomic_swap(p: *mut u64, expected: u64, new: u64) -> u64",
                );
                return Some(Type::Error);
            }
            let pt = ck.expr(&args[0], Some(&Type::ptr(Type::U64, true)));
            let et = ck.expr(&args[1], Some(&Type::U64));
            let nt = ck.expr(&args[2], Some(&Type::U64));
            if !pt.is_error() && !is_u64_ptr(&pt) {
                ck.dg.error_note(
                    args[0].span,
                    format!(
                        "'{}' expects a *mut u64 as first argument, found {}",
                        CAS,
                        ck.tcx.name_of(&pt)
                    ),
                    "exactly one 64-bit word is swapped atomically",
                );
                return Some(Type::Error);
            }
            for (t, i) in [(&et, 1usize), (&nt, 2usize)] {
                if !t.is_error() && !fits_as_u64(t) {
                    ck.dg.error(
                        args[i].span,
                        format!("'{}' expects a u64, found {}", CAS, ck.tcx.name_of(t)),
                    );
                    return Some(Type::Error);
                }
            }
            Some(Type::U64)
        }
    }
}

fn is_u64_ptr(t: &Type) -> bool {
    match t {
        Type::Ptr { inner, .. } => **inner == Type::U64,
        _ => false,
    }
}

fn is_ptr(t: &Type) -> bool {
    matches!(t, Type::Ptr { .. })
}

fn fits_as_u64(t: &Type) -> bool {
    matches!(t, Type::U64 | Type::UntypedInt | Type::I64 | Type::Usize)
}

// ------------------------------------------------------------ Lowering phase

/// Hook from `lower::lower_call`.
pub(crate) fn lower_thread_call(
    lo: &mut Lower,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Option<Option<Val>> {
    match name {
        SELF => Some(Some(lo.push(FTy::Ptr, Op::ThreadSelf))),
        START => {
            if args.len() != 3 {
                return lo.ice(span, "thread primitive with wrong arity");
            }
            let a = lo.lower_expr(&args[0])?;
            let s = lo.lower_expr(&args[1])?;
            let c = lo.lower_expr(&args[2])?;
            Some(Some(lo.push(FTy::I64, Op::ThreadSpawn { arg: a, stack: s, ctid: c })))
        }
        _ => {
            if args.len() != 3 {
                return lo.ice(span, "atomic swap with wrong arity");
            }
            let p = lo.lower_expr(&args[0])?;
            let e = lo.lower_expr(&args[1])?;
            let n = lo.lower_expr(&args[2])?;
            Some(Some(lo.push(FTy::U64, Op::AtomicCas { addr: p, erw: e, new: n })))
        }
    }
}

// ------------------------------------------------------------- Code generator

/// The instruction sequence for `Op::ThreadSpawn`. Precondition: `rdi` =
/// argument, `rsi` = upper stack address, `rdx` = pointer to the TID word.
/// Postcondition: `rax` = thread id (> 0) or negative error value.
///
/// The local label `1:` is a **numeric** label of the assembler: `jnz 1f`
/// jumps forward to the next `1:`. That way this sequence needs no counter
/// and may appear as often as you like in the same module.
/// **ROUND WINDOWS** — the same instruction where there is no `clone(2)`.
///
/// The postcondition of `spawn_sequence` is "`rax` = thread id (> 0) or a
/// negative error value", and that second half is what makes an honest
/// answer possible without a second mechanism: `-38` is `ENOSYS`, the
/// number the seam gives for every system call Windows has no counterpart
/// for. A program that never starts a thread never reaches this
/// instruction; one that does gets a failure it can read, at the line
/// where it started the thread.
pub(crate) fn spawn_unsupported(e: &mut crate::codegen_x86::Emitter) {
    e.raw("    # windows: clone(2) has no equivalent -- ENOSYS");
    e.line("mov rax, -38");
}

pub(crate) fn spawn_sequence(e: &mut crate::codegen_x86::Emitter) {
    // Put the argument on the CHILD stack — in the child `rdi` is overwritten
    // with the flags, and there is no other way to reach the value.
    // 16 bytes, so that the stack stays aligned (SysV: 16-fold aligned at the
    // `call` site).
    e.line("sub rsi, 16");
    e.line("mov qword ptr [rsi], rdi");
    e.line("mov r10, rdx");
    e.line(&format!("mov rdi, {}", CLONE_FLAGS));
    e.line("xor r8d, r8d");
    e.line("mov eax, 56");
    e.line("syscall");
    e.line("test rax, rax");
    e.line("jnz 1f");
    // ---- child: own stack, `rbp` fresh, fetch the argument back ---------
    e.line("mov rdi, qword ptr [rsp]");
    e.line("add rsp, 16");
    e.line("xor ebp, ebp");
    e.line(&format!("call {}", crate::codegen_x86::label(ENTRY)));
    // exit(2), NOT exit_group(2): this thread alone ends.
    e.line("mov edi, eax");
    e.line("mov eax, 60");
    e.line("syscall");
    e.line("ud2");
    e.raw("1:");
}

/// The same for AArch64. Precondition: `x0` = argument, `x1` = child stack,
/// `x2` = the TID word. Postcondition: `x0` = thread id (creator) resp. the
/// child never comes back.
///
/// **The one real difference to x86-64** and the reason `syscalls.rs` refuses
/// to translate `clone` by its number alone: the generic system call table
/// swaps two of the arguments.
///
/// ```text
/// x86-64   clone(flags, stack, parent_tid, child_tid, tls)
/// aarch64  clone(flags, stack, parent_tid, tls, child_tid)
/// ```
///
/// `tls` stays 0 here; the child sets its own thread pointer as its first
/// act (`__thread_entry` -> `arch_prctl(ARCH_SET_FS)` -> `msr tpidr_el0`).
pub(crate) fn spawn_sequence_a64(e: &mut crate::codegen_x86::Emitter) {
    let back = format!(".La64_spawn_{}", e.out.len());
    // The argument goes onto the CHILD stack: in the child `x0` is the
    // return value of the system call, and there is no other way to reach
    // the value. 16 bytes, so the stack stays 16-aligned (AAPCS64 §6.2.2).
    e.line("sub x1, x1, #16");
    e.line("str x0, [x1]");
    // x2 already holds the TID word and is the parent_tid argument; the
    // child_tid argument is the same word (CLONE_PARENT_SETTID and
    // CLONE_CHILD_CLEARTID both point at it, exactly as on x86).
    e.line("mov x4, x2"); // child_tid
    e.line("mov x3, xzr"); // tls -- the child sets its own
    crate::codegen_a64::imm_into(e, "x0", CLONE_FLAGS as i64);
    crate::codegen_a64::imm_into(e, "x8", 220);
    e.line("svc #0");
    e.line(&format!("cbnz x0, {}", back));
    // ---- child: own stack, fetch the argument back ----------------------
    e.line("ldr x0, [sp]");
    e.line("add sp, sp, #16");
    e.line("mov x29, xzr");
    e.line(&format!("bl {}", crate::codegen_x86::label(ENTRY)));
    // exit(2), NOT exit_group(2): this thread alone ends.
    e.line("mov w0, w0");
    e.line("mov x8, #93");
    e.line("svc #0");
    e.line("brk #0");
    e.raw(&format!("{}:", back));
}

/// The instruction sequence for `Op::AtomicCas`. Precondition: `rcx` =
/// address, `rax` = expected value, `rdx` = new value. Postcondition:
/// `rax` = the value found (equal to the expectation if the swap happened).
pub(crate) fn cas_sequence(e: &mut crate::codegen_x86::Emitter) {
    e.line("lock cmpxchg qword ptr [rcx], rdx");
}

/// The instruction sequence for `Op::ThreadSelf`: the self pointer out of
/// the thread block. Precondition: none. Postcondition: `rax` = thread block.
pub(crate) fn self_sequence(e: &mut crate::codegen_x86::Emitter) {
    e.line("mov rax, qword ptr fs:0");
}
