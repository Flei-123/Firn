// SPDX-License-Identifier: GPL-2.0-only
//! Real register allocation: **linear scan with liveness intervals**
//! (Poletto/Sarkar) plus a register aware emission path.
//!
//! Up to round 1 every FIR value got a stack slot of its own; every
//! instruction was `load`-`load`-compute-`store`. This file replaces that:
//!
//!  1. **Liveness analysis** per basic block (backward flow, `live_in`/
//!     `live_out`), and from it ONE interval `[start, end]` per value in a
//!     linear numbering of all instructions.
//!  2. **Cell promotion:** an `alloca` whose pointer never escapes (used only
//!     as the direct address of a `load`/`store`), that is at most 8 bytes
//!     big and that is always accessed at the same width, lives entirely in
//!     a register — `load` becomes a register copy, `store` a register write.
//!     That replaces the phi nodes which FIR does not have, and it brings
//!     exactly those loop counters into registers that `mem2reg` (cells
//!     written only once) cannot promote.
//!  3. **Linear scan** over the intervals sorted by `start`, with an active
//!     list; if the supply does not suffice, the interval with the latest end
//!     and the smallest weight (uses, weighted by loop depth) is spilled to
//!     the stack. It is NOT split: a value lives either in a register for
//!     its whole lifetime or on the stack for its whole lifetime — that way
//!     no reload logic is needed and the allocation is provably behaviour
//!     preserving.
//!
//! **Register choice (System V AMD64):**
//!  * `rax`, `rcx`, `rdx`, `rsi`, `rdi` stay scratch registers and are never
//!    handed out (they are the argument/helper registers of `call`, `syscall`,
//!    `div`, `rep movsb`).
//!  * Handed out are `rbx`, `r12`, `r13`, `r14`, `r15` (callee-saved, saved in
//!    the prologue/epilogue) and `r11` (caller-saved) — `r11` only for
//!    intervals that span no `call`/`syscall`.
//!  * `r10`, `r8`, `r9` are deliberately NOT handed out: they are argument
//!    registers of `call`/`syscall` and could overwrite a value that is still
//!    needed while the argument list is being built.
//!
//! **SPEC §9:** `Op::Select` stays `cmov`, `Op::Barrier` and `Op::SecureZero`
//! are emitted unchanged, and the check "conditional jump depends on a
//! `secret` value" holds on this path just as on the base path.
//!
//! Since round 43 this path can handle **more than six parameters or
//! arguments** too (System V: from the seventh one on via the stack). Before that
//! every function containing such a call fell back to the base path — in the
//! tokenizer measurement run those were `main`, `tok_emit`, `sink_flush_chars`,
//! `sink_end`, `out_error_list` and `out_word`, together a quarter of all
//! instructions executed.
//!
//! The emission path stays **guarded**: constructs that it does not handle
//! completely (`f64`, unknown block numbering …) make `emit_func_ra` return
//! `None`, so that `codegen_x86.rs` falls back to the base path that has
//! proven itself.

use crate::codegen_x86::{block_label, label, size_word, Emitter, Frame, ARG_REGS};
use crate::fir::{BinOp, Block, BlockId, CmpOp, FTy, Func, Inst, Op, Term, UnOp, Val};
use std::collections::HashMap;

/// Place of a value after the allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loc {
    /// fixed machine register (64-bit name)
    Reg(&'static str),
    /// stack slot: address = `rbp - off`
    Slot(u64),
}

/// Does the value `v` write into the physical register `r`? (Round 41 — check
/// for the cell alias.)
fn used_register(alloc: &Alloc, v: Val, r: &'static str) -> bool {
    if let Some(rc) = alloc.cells.get(&v) {
        if *rc == r {
            return true;
        }
    }
    matches!(alloc.locs.get(v as usize), Some(Loc::Reg(x)) if *x == r)
}

/// callee-saved registers that may get handed out (prologue/epilogue save).
const CALLEE_SAVED: [&str; 5] = ["rbx", "r12", "r13", "r14", "r15"];
/// caller-saved register for intervals that enclose NO `call`/`syscall`:
/// in that case neither the call itself nor the build-up of its argument
/// list (rdi, rsi, rdx, rcx, r8, r9, r10) can destroy the value.
const TEMP_REGS: [&str; 4] = ["r11", "r10", "r9", "r8"];
/// Argument registers that become free as long as the interval crosses no
/// `call`/`syscall` and no `copymem`/`secure_zero` (see `Iv`).
const ARG_SPARE: [&str; 2] = ["rsi", "rdi"];
/// On top of that `rdx` is used by `div`/`rem`/`select` as a scratch
/// register — only intervals that cross none of this may carry it.
const DIV_SPARE: [&str; 1] = ["rdx"];

// ------------------------------------------- implicit clobbers (round 90) ---
//
// ROUND 90 — THE BUG THIS SECTION EXISTS FOR, AND WHY IT IS A SECTION.
//
// `firnc --opt-level=release-safe` produced WRONG CODE. The minimal case is
// `tests/1900_mul_clobbers_rdx.fi`; it was found by the osum kernel, whose
// bitmap frame allocator failed 19 of 23 of its own cases on that build
// level and on no other:
//
//     a6:  mov  %rcx,%rdx      # the fourth parameter lives in rdx from here
//     df:  mul  %rcx           # <-- writes RDX:RAX. rdx is gone.
//
// x86 has instructions that write registers their operand list never
// mentions. `mul`/`imul` in the ONE operand form put the full product in
// `rdx:rax`, `div`/`idiv` take their dividend from there and leave the
// remainder in `rdx`, `cqo`/`cdq` sign-extend into `rdx`. The allocator hands
// `rdx` out as a value register. If it does not know that an instruction
// destroys it, the value is destroyed.
//
// It DID know for `div`/`rem`/`select`/`cmpxchg` (`divsel_pos` below) — and
// round 72, which introduced checked arithmetic, added its four new
// instructions to that list. Round 87 then built `exact_crossings`, a second,
// finer answer to the same question, and listed only the ops round 49 knew
// about. Two lists, one question, and the finer one silently won: everything
// round 72 added became invisible again. `--opt-level=release-safe` is the
// only level that has both halves at once (checked arithmetic makes the
// `mul`, register allocation makes the victim), which is why it alone broke —
// and it is exactly the level one ships with.
//
// The answer is not a third list. It is ONE function, [`inst_clobbers`],
// that says for every instruction which registers OUT OF THE POOL its code
// destroys, and one representation, a bit mask, that every consumer uses.
// The rough interval answer and the exact control-flow answer are now two
// ways of summing the SAME masks, and `fits` is one line: does this register
// appear in what dies while the value is alive. A new instruction that
// clobbers something can be forgotten in exactly one place instead of three,
// and forgetting it there is a compile error at the `match` if it is a new
// `Op` variant.
//
// The masks are also NARROWER than the three booleans they replace, which is
// worth registers on `release-safe`: a checked `+` or `-` writes no `rdx` at
// all (only the unsigned one-operand `mul` does), a checked `as` writes none
// either, and `copymem` writes `rdi`/`rsi` but not `rdx` — all three used to
// ban `rdx` wholesale through `crosses_divsel`/`crosses_memop`.
//
/// The registers this allocator ever hands to a value, in one order that
/// every mask in this file uses. `rax`/`rcx` are NOT in it: they are pure
/// scratch at every emission site and can never hold a FIR value.
const POOL: [&str; 12] = [
    "rbx", "r12", "r13", "r14", "r15", "r11", "r10", "r9", "r8", "rsi", "rdi", "rdx",
];

/// A set of [`POOL`] registers.
type RegMask = u16;

const M_RBX: RegMask = 1 << 0;
const M_R12: RegMask = 1 << 1;
const M_R13: RegMask = 1 << 2;
const M_R14: RegMask = 1 << 3;
const M_R15: RegMask = 1 << 4;
const M_R11: RegMask = 1 << 5;
const M_R10: RegMask = 1 << 6;
const M_R9: RegMask = 1 << 7;
const M_R8: RegMask = 1 << 8;
const M_RSI: RegMask = 1 << 9;
const M_RDI: RegMask = 1 << 10;
const M_RDX: RegMask = 1 << 11;

/// Everything a `call` destroys. The five callee-saved ones survive it (the
/// prologue/epilogue of the callee saves them), which is why they are not in
/// here and why an interval that crosses a call can still get one.
const M_CALL: RegMask = M_R11 | M_R10 | M_R9 | M_R8 | M_RSI | M_RDI | M_RDX;
/// `rep movsb`/`rep stosb`: `rdi`, `rsi` (and `rcx`, which is not in the pool).
const M_MEMOP: RegMask = M_RDI | M_RSI;

/// Bit of a pool register; 0 for anything that is not in the pool.
fn reg_bit(r: &str) -> RegMask {
    match r {
        "rbx" => M_RBX,
        "r12" => M_R12,
        "r13" => M_R13,
        "r14" => M_R14,
        "r15" => M_R15,
        "r11" => M_R11,
        "r10" => M_R10,
        "r9" => M_R9,
        "r8" => M_R8,
        "rsi" => M_RSI,
        "rdi" => M_RDI,
        "rdx" => M_RDX,
        _ => 0,
    }
}

/// **THE SINGLE SOURCE OF TRUTH** — which pool registers does the code that
/// this backend emits for `i` destroy?
///
/// Only the path that RETURNS counts. A checked operation's overflow arm
/// jumps to the panic trampoline and never comes back, so the `pop rdx` it
/// does on the way out is not a clobber anybody can observe.
///
/// The width and signedness questions are not cosmetic: `mul cl` (8 bit
/// unsigned) puts its product in `ax` and leaves `rdx` alone, `imul rax, rcx`
/// (the two operand form, every signed multiplication at 16 bits and wider)
/// writes only its target, and only `mul cx`/`ecx`/`rcx` really splits the
/// product across `rdx:rax`.
fn inst_clobbers(i: &Inst) -> RegMask {
    let ty = i.ty;
    match &i.op {
        // A call and everything shaped like one. `syscall` itself only
        // destroys rax/rcx/r11 architecturally, but building its argument
        // list writes rdi, rsi, rdx, r10, r8 and r9 first.
        Op::Call { .. } | Op::CallIndirect { .. } | Op::Syscall { .. } | Op::ThreadSpawn { .. } => {
            M_CALL
        }
        // `__cpu_features()` — `cpuid` writes rax/rbx/rcx/rdx and the sequence
        // in `simd.rs` uses r9/r10/r11 on top (it saves and restores rbx
        // itself). See `is_cpuid`: it counts as a call, which is what round 87
        // already decided after tests/1613_crypto.fi died of it.
        Op::Simd { .. } if is_cpuid(&i.op) => M_CALL,
        // `rep movsb` / `rep stosb`.
        Op::CopyMem { .. } | Op::SecureZero { .. } => M_MEMOP,
        // `cqo`/`cdq` + `div`/`idiv`: the remainder register.
        Op::Bin(BinOp::Div | BinOp::Rem, _, _) => M_RDX,
        // `test dl, dl` + `cmovnz` — the condition is fetched into rdx BEFORE
        // the two arms are read (see `Op::Select` in the emission below).
        Op::Select { .. } => M_RDX,
        // `lock cmpxchg [rcx], rdx` (round 49, found in tests/820).
        Op::AtomicCas { .. } => M_RDX,
        // Checked `/` and `%` always divide in the end, and the signed
        // 64-bit range test parks `i64::MIN` in rdx to compare against.
        Op::CheckedDiv { .. } => M_RDX,
        // Checked `+ - *`: ONLY the unsigned one-operand `mul` at 16 bits and
        // wider touches rdx. THIS is the instruction of the bug.
        Op::CheckedBin { op: BinOp::Mul, .. } if !ty.signed() && ty.bits() >= 16 => M_RDX,
        Op::CheckedBin { .. } => 0,
        // Checked `as` — ROUND 90: `mov rdx, rax` instead of `push rax`,
        // so the round trip has something to compare against without
        // touching memory. That makes rdx a clobber where round 72 had
        // none; the stack traffic it replaces was two accesses per cast.
        Op::CheckedCast { .. } => M_RDX,
        // ROUND 89's checked index, merged into round 90's single list.
        // `emit_checked_idx` is `cmp rax, imm32` / `jb` / `mov rcx, len`
        // and the failure arm never returns -- rax and rcx are the two
        // scratch registers, rdx is untouched. Written out rather than
        // left to the `_` arm so the next reader does not have to go and
        // look it up in panic_rt.rs.
        Op::CheckedIdx { .. } => 0,
        // `+% -% *%` is bit for bit the unchecked path (two operand `imul`).
        // `+| -| *|` clamps with `mov rdx, <bound>` + `cmovl` for the signed
        // types, and multiplies through the one-operand `mul` for the
        // unsigned ones.
        Op::BinWrapSat { kind, op, .. } => {
            if *kind == crate::fir::WrapSatKind::Wrap {
                0
            } else if ty.signed() {
                M_RDX
            } else if *op == BinOp::Mul && ty.bits() >= 16 {
                M_RDX
            } else {
                0
            }
        }
        // Everything else computes in rax/rcx or in the target register.
        // `lock xadd [rcx], rax`, `crc32 eax, cl`, `setcc al`, the shifts
        // (count in cl), `Op::Cmp`, loads, stores, `lea` — none of them
        // reaches past the two scratch registers.
        _ => 0,
    }
}

/// Operands that an instruction fetches into a FIXED register while another
/// fixed register it writes is still to be read — see the long note in
/// [`exact_crossings`]. They must be treated as living ACROSS the
/// instruction even though their interval ends at it.
fn op_pins(op: &Op, out: &mut Vec<Val>) {
    out.clear();
    match op {
        Op::CallIndirect { target, .. } => out.push(*target),
        Op::Syscall { args } => {
            if let Some(a0) = args.first() {
                out.push(*a0);
            }
        }
        Op::ThreadSpawn { arg, stack, ctid } => {
            out.push(*arg);
            out.push(*stack);
            out.push(*ctid);
        }
        Op::CopyMem { dst, src, .. } => {
            out.push(*dst);
            out.push(*src);
        }
        Op::SecureZero { addr, size } => {
            out.push(*addr);
            out.push(*size);
        }
        Op::Select { cond, a, b } => {
            out.push(*cond);
            out.push(*a);
            out.push(*b);
        }
        Op::AtomicCas { addr, erw, new } => {
            out.push(*addr);
            out.push(*erw);
            out.push(*new);
        }
        // ROUND 90 — the checked operations. Their failure arm RELOADS the
        // two original values from wherever they live (`panic_rt.rs`), so
        // those homes have to survive the instruction: an operand may not
        // sit in a register the instruction itself destroys. Only matters
        // where the mask is non-empty (the unsigned one-operand `mul`, the
        // divisions, the saturating clamp, the cast's compare register) --
        // `exact_crossings` never asks for pins where nothing is clobbered.
        Op::CheckedBin { a, b, .. } | Op::CheckedDiv { a, b, .. } => {
            out.push(*a);
            out.push(*b);
        }
        Op::CheckedCast { src, .. } => out.push(*src),
        Op::BinWrapSat { a, b, .. } => {
            out.push(*a);
            out.push(*b);
        }
        _ => {}
    }
}

fn align_up(x: u64, a: u64) -> u64 {
    if a <= 1 {
        x
    } else {
        (x + a - 1) / a * a
    }
}

/// Result of the register allocation of a function.
pub struct Alloc {
    locs: Vec<Loc>,
    /// load results that read their value directly from the cell register
    /// (cell alias, round 40): val -> cell register
    alias: HashMap<Val, &'static str>,
    /// val -> the promoted cell it was loaded from
    alias_src: HashMap<Val, Val>,
    /// Constants that may appear as an immediate operand at EVERY one of their
    /// use sites: they need neither a register nor a slot.
    imms: HashMap<Val, i64>,
    /// `alloca` values with a fixed frame offset (addressing without a detour).
    frame_addr: HashMap<Val, u64>,
    /// promoted `alloca` cells: pointer value -> register
    cells: HashMap<Val, &'static str>,
    /// access width per promoted cell
    cell_ty: HashMap<Val, FTy>,
    /// callee-saved registers used and their save slot
    saved: Vec<(&'static str, u64)>,
    frame: Frame,
    /// Round 87: why did the values that got no register not get one? Only
    /// filled when `FIRN_RA_STATS` is set -- the counting costs nothing, but
    /// the exact crossings do, and nobody should pay for them in a normal
    /// build.
    stats: Option<RaStats>,
}

/// ROUND 87 -- the cause distribution behind the one number "spilled".
///
/// `docs/BENCHMARKS.md` said "spills more than half the values" and left it
/// at that. Half of what, and WHY, decides what has to be built. These are
/// the four possible answers, and they are counted separately:
///
///   * `no_interval` -- the value is never touched. Dead code that no pass
///     removed. It has a slot and never sees it; that is not a spill.
///   * `secret` -- must stay in memory, SPEC 9.2. Not a spill either.
///   * `lost_call` -- the interval crosses a call, so only the five
///     callee-saved registers were possible, and all five were taken.
///   * `lost_plain` -- crosses no call, and all twelve were taken anyway.
///   * `evicted` -- had a register and lost it to a heavier interval.
///
/// `cross_call` against `cross_call_exact` is the finding of the round: how
/// many of the intervals that the allocator BELIEVES cross a call really do.
#[derive(Default, Clone, Copy)]
pub(crate) struct RaStats {
    pub ivs: usize,
    pub cross_call: usize,
    pub cross_call_exact: usize,
    pub no_interval: usize,
    pub secret: usize,
    pub lost_call: usize,
    pub lost_plain: usize,
    pub evicted: usize,
    pub cells_lost: usize,
    pub cell_ivs: usize,
    pub max_live: usize,
}

impl Alloc {
    /// Place of a value. The only query interface of the code generator.
    pub fn loc(&self, v: Val) -> Loc {
        self.locs.get(v as usize).copied().unwrap_or(Loc::Slot(0))
    }
    /// Place of a value as a SOURCE: a load result with a cell alias lies
    /// nowhere, its value is in the cell register. For TARGETS `loc()` holds.
    pub fn place(&self, v: Val) -> Loc {
        if let Some(r) = self.alias.get(&v) {
            return Loc::Reg(r);
        }
        self.loc(v)
    }
    /// Immediate operand of a value, if it is suitable as one.
    fn imm(&self, v: Val) -> Option<i64> {
        self.imms.get(&v).copied()
    }
    /// Register of a promoted `alloca` cell, if there is one.
    fn cell(&self, addr: Val) -> Option<(&'static str, FTy)> {
        match (self.cells.get(&addr), self.cell_ty.get(&addr)) {
            (Some(r), Some(t)) => Some((*r, *t)),
            _ => None,
        }
    }
}

// ------------------------------------------------------------ Frame layout ---

fn layout(f: &Func, extra_slots: u64) -> (Frame, Vec<(&'static str, u64)>) {
    let n = f.val_types.len();
    let mut slot = vec![0u64; n];
    let mut cursor = 0u64;
    for s in slot.iter_mut() {
        cursor += 8;
        *s = cursor;
    }
    let mut alloca_off: Vec<Option<u64>> = vec![None; n];
    for b in &f.blocks {
        if b.id != f.entry() && b.insts.iter().any(|i| matches!(i.op, Op::Alloca { .. })) {
            continue;
        }
        for i in &b.insts {
            if let Op::Alloca { size, align } = i.op {
                if let Some(d) = i.dst {
                    let a = if align == 0 { 1 } else { align.min(16) };
                    cursor = align_up(cursor + size.max(1), a);
                    alloca_off[d as usize] = Some(cursor);
                }
            }
        }
    }
    let mut saved = Vec::new();
    for k in 0..extra_slots {
        cursor += 8;
        let _ = k;
        saved.push(cursor);
    }
    let saved_pairs: Vec<(&'static str, u64)> =
        saved.into_iter().map(|off| ("", off)).collect::<Vec<_>>();
    (Frame { slot, alloca_off, size: align_up(cursor, 16) }, saved_pairs)
}

// ----------------------------------------------------------- Liveness analysis ---

struct Live {
    /// linear position of the first instruction per block
    block_start: Vec<usize>,
    /// position of the terminator per block
    block_end: Vec<usize>,
    /// position of every instruction: pos[block][index]
    pos: Vec<Vec<usize>>,
    live_in: Vec<Vec<bool>>,
    live_out: Vec<Vec<bool>>,
    /// Did the data flow reach its fixed point? (round 87 -- the loop has a
    /// round limit, and below that limit the sets may be TOO SMALL. Anything
    /// finer than the interval bounds may then not be derived from them.)
    converged: bool,
}

fn compute_live(f: &Func) -> Live {
    let nb = f.blocks.len();
    let nv = f.val_types.len();
    let mut pos = Vec::with_capacity(nb);
    let mut block_start = vec![0usize; nb];
    let mut block_end = vec![0usize; nb];
    let mut p = 1usize;
    for (bi, b) in f.blocks.iter().enumerate() {
        block_start[bi] = p;
        let mut v = Vec::with_capacity(b.insts.len());
        for _ in &b.insts {
            v.push(p);
            p += 1;
        }
        block_end[bi] = p;
        p += 1;
        pos.push(v);
    }

    let mut usek = vec![vec![false; nv]; nb];
    let mut defk = vec![vec![false; nv]; nb];
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for &u in buf.iter() {
                if (u as usize) < nv && !defk[bi][u as usize] {
                    usek[bi][u as usize] = true;
                }
            }
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    defk[bi][d as usize] = true;
                }
            }
        }
        let t = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = t {
            if (v as usize) < nv && !defk[bi][v as usize] {
                usek[bi][v as usize] = true;
            }
        }
    }

    let mut live_in = vec![vec![false; nv]; nb];
    let mut live_out = vec![vec![false; nv]; nb];
    let mut rounds = 0usize;
    let mut converged = true;
    loop {
        rounds += 1;
        let mut changed = false;
        for bi in (0..nb).rev() {
            let mut out = vec![false; nv];
            for s in f.blocks[bi].term.successors() {
                let s = s as usize;
                if s < nb {
                    for v in 0..nv {
                        out[v] |= live_in[s][v];
                    }
                }
            }
            if out != live_out[bi] {
                live_out[bi] = out;
                changed = true;
            }
            let mut inn = vec![false; nv];
            for v in 0..nv {
                inn[v] = usek[bi][v] || (live_out[bi][v] && !defk[bi][v]);
            }
            if inn != live_in[bi] {
                live_in[bi] = inn;
                changed = true;
            }
        }
        if !changed {
            break;
        }
        if rounds > nb + 4 {
            // Did NOT converge. The caller must not draw any conclusion from
            // these sets that is finer than the interval bounds (round 87).
            converged = false;
            break;
        }
    }
    Live { block_start, block_end, pos, live_in, live_out, converged }
}

// ------------------------------------------------- exact crossings (r87) ---
//
// ROUND 87 -- WHY THIS EXISTS, and it is the finding of the round.
//
// `crosses_call` used to be asked as "does a call position lie between the
// first and the last touch of the value". That is the INTERVAL, and the
// interval is a straight line through a graph: the blocks are numbered, and
// everything numbered in between counts as "in between" even when no path
// from the definition to the use runs through it at all.
//
// A value that crosses a call may only have one of the five callee-saved
// registers. Every false positive here therefore costs seven of twelve
// registers and pushes the value onto the stack once those five are taken.
//
// This function asks the question exactly: a value crosses the call at `p`
// exactly when it is live BEFORE `p` and live AFTER `p`. That is what
// "survives the call" means, it is the textbook definition of live-through,
// and it follows the control flow instead of the block numbering. An
// argument that dies at the call is live before and dead after -- it does
// not survive it. The result of the call is dead before and live after --
// it does not survive it either.
//
// Sound in the other direction as well: two intervals that both hold `r10`
// still never overlap, because the linear scan keeps using the CLOSED
// interval `[start,end]` for that. This function only ever removes a
// RESTRICTION, it never shortens a lifetime.
//
// The bitsets are words, not `Vec<bool>`: `gctext__gctext_write` has 56,359
// values, and one pass over a `Vec<bool>` per call instruction would have
// cost more than the whole allocation.
struct Bits {
    w: Vec<u64>,
}

impl Bits {
    fn new(n: usize) -> Bits {
        Bits { w: vec![0u64; n / 64 + 1] }
    }
    fn from_bools(b: &[bool]) -> Bits {
        let mut s = Bits::new(b.len());
        for (i, &x) in b.iter().enumerate() {
            if x {
                s.w[i >> 6] |= 1u64 << (i & 63);
            }
        }
        s
    }
    fn set(&mut self, i: usize) {
        self.w[i >> 6] |= 1u64 << (i & 63);
    }
    fn clear(&mut self, i: usize) {
        self.w[i >> 6] &= !(1u64 << (i & 63));
    }
    fn get(&self, i: usize) -> bool {
        self.w[i >> 6] & (1u64 << (i & 63)) != 0
    }
    fn copy_from(&mut self, o: &Bits) {
        self.w.copy_from_slice(&o.w);
    }
    fn fill_from_bools(&mut self, b: &[bool]) {
        for x in self.w.iter_mut() {
            *x = 0;
        }
        for (i, &x) in b.iter().enumerate() {
            if x {
                self.w[i >> 6] |= 1u64 << (i & 63);
            }
        }
    }
    /// `self |= a & b`
    #[allow(dead_code)]
    fn or_and(&mut self, a: &Bits, b: &Bits) {
        for i in 0..self.w.len() {
            self.w[i] |= a.w[i] & b.w[i];
        }
    }
}

/// `__cpu_features()` -- ROUND 87, A BUG OF ROUND 82.
///
/// `unsupported_basic` lets three vector instructions through to this
/// allocating path, among them `__cpu_features()`. Its `cpuid` sequence in
/// `simd.rs` writes `r9`, `r10` and `r11` and says so in its own comment:
/// "carry no value ON THE BASE PATH of the code generator". On THIS path
/// they do -- all three are `TEMP_REGS`.
///
/// Found in tests/1613_crypto.fi: `sha256_new` held the address of its
/// `accel` flag in `r9` across the sequence, `xor r9d, r9d` erased it, and
/// the write afterwards went to address 0. The bug is older than this round;
/// the exact crossing question merely made it happen every time instead of
/// depending on the day. Values that survive the sequence therefore count as
/// surviving a call, and only the callee-saved registers remain -- `rbx` is
/// pushed and popped by the sequence itself.
fn is_cpuid(op: &Op) -> bool {
    matches!(op, Op::Simd { kind: crate::simd::SimdKind::CpuFeatures, .. })
}

/// Calls `f` for every index that is set in BOTH bitsets. Word by word:
/// `gctext__gctext_write` has 56,359 values, and a pass over a `Vec<bool>`
/// per clobbering instruction would cost more than the whole allocation.
fn each_and(a: &Bits, b: &Bits, mut f: impl FnMut(usize)) {
    for (i, (x, y)) in a.w.iter().zip(b.w.iter()).enumerate() {
        let mut m = x & y;
        while m != 0 {
            f(i * 64 + m.trailing_zeros() as usize);
            m &= m - 1;
        }
    }
}

/// For every value: WHICH pool registers really die while it is alive?
/// Exact, along the control flow, using [`inst_clobbers`] as its only table.
fn exact_crossings(f: &Func, live: &Live) -> Vec<RegMask> {
    let nv = f.val_types.len();
    let mut killed = vec![0 as RegMask; nv];
    let mut after = Bits::new(nv);
    let mut cur = Bits::new(nv);
    let mut buf = Vec::new();
    let mut pins = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        cur.fill_from_bools(&live.live_out[bi]);
        // The terminator runs AFTER the last instruction and reads its value
        // there; `live_out` is the state after the terminator.
        let tv = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = tv {
            if (v as usize) < nv {
                cur.set(v as usize);
            }
        }
        for i in b.insts.iter().rev() {
            let m = inst_clobbers(i);
            if m != 0 {
                after.copy_from(&cur);
            }
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    cur.clear(d as usize);
                }
            }
            buf.clear();
            i.op.uses(&mut buf);
            for &u in buf.iter() {
                if (u as usize) < nv {
                    cur.set(u as usize);
                }
            }
            if m != 0 {
                // Live BEFORE and live AFTER is the textbook definition of
                // live-through: an operand that dies here does not survive
                // the instruction, the result does not exist before it.
                each_and(&cur, &after, |v| killed[v] |= m);
                // THE OTHER HALF, and it cost a segmentation fault to find:
                // "does not survive the instruction" is not the same as "may
                // stand in any register at it". A few instructions fetch
                // their operands into FIXED registers ONE AFTER THE OTHER,
                // and the second fetch overwrites the home of the first.
                //
                //   call rax          the target is loaded LAST, after
                //                     rdi..r9 have been set -- a target
                //                     living in `rdi` is gone by then
                //                     (measured: tests/1402 jumped to 0x10).
                //   syscall           the number goes into rax last, likewise.
                //   rep movsb         rdi, then rsi: a source living in `rdi`
                //                     is overwritten by the destination.
                //   cmov / cmpxchg    rdx is written before the other two
                //                     operands are read.
                //
                // The interval question used to hide all of that: an
                // operand's interval ENDS at the instruction, so it counted
                // as crossing it anyway. The exact question sees it die there
                // and would hand it exactly the register about to be
                // overwritten. So these operands are pinned by hand.
                //
                // Deliberately NOT in the list: the arguments of a normal
                // `call` and of a `syscall`. Those go through
                // `parallel_reg_moves`, which resolves any permutation and
                // breaks cycles over `rax` -- and they are the big group,
                // several thousand values.
                op_pins(&i.op, &mut pins);
                for &pv in pins.iter() {
                    if (pv as usize) < nv {
                        killed[pv as usize] |= m;
                    }
                }
            }
        }
    }
    killed
}

// ---------------------------------------------------------- Cell analysis ---

/// Finds `alloca`s that can live entirely in a register.
fn promotable_cells(f: &Func) -> HashMap<Val, FTy> {
    let mut cand: HashMap<Val, Option<FTy>> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { size, .. }) = (i.dst, &i.op) {
                if *size <= 8 && !f.is_secret(d) {
                    cand.insert(d, None);
                }
            }
        }
    }
    if cand.is_empty() {
        return HashMap::new();
    }
    let mut bad: Vec<Val> = Vec::new();
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                Op::Load { addr } => {
                    if let Some(slot) = cand.get_mut(addr) {
                        match slot {
                            Some(t) if *t != i.ty => bad.push(*addr),
                            Some(_) => {}
                            None => *slot = Some(i.ty),
                        }
                    }
                    if let Some(d) = i.dst {
                        if f.is_secret(d) && cand.contains_key(addr) {
                            bad.push(*addr);
                        }
                    }
                }
                Op::Store { addr, val } => {
                    if let Some(slot) = cand.get_mut(addr) {
                        match slot {
                            Some(t) if *t != i.ty => bad.push(*addr),
                            Some(_) => {}
                            None => *slot = Some(i.ty),
                        }
                    }
                    if cand.contains_key(val) {
                        bad.push(*val); // the address escapes as a value
                    }
                }
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    for v in buf.iter() {
                        if cand.contains_key(v) {
                            bad.push(*v);
                        }
                    }
                }
            }
        }
        match &b.term {
            Term::Ret(Some(v)) | Term::BrCond { cond: v, .. } | Term::Switch { val: v, .. } => {
                if cand.contains_key(v) {
                    bad.push(*v);
                }
            }
            _ => {}
        }
    }
    for b in bad {
        cand.remove(&b);
    }
    cand.into_iter().filter_map(|(v, t)| t.map(|t| (v, t))).collect()
}


// ------------------------------------ Immediate constants / direct addressing ---

/// Constants that may appear as an x86 immediate operand at EVERY use site.
/// They then need neither register nor slot, and their `const` instruction
/// disappears entirely.
fn immediate_consts(f: &Func) -> HashMap<Val, i64> {
    // ROUND 92 -- A VALUE MAY BE WRITTEN MORE THAN ONCE DOWN HERE.
    //
    // Above the code generator FIR is SSA: one definition per value, so
    // "this value is defined by a `const`" and "this value IS that constant"
    // were the same sentence. `phi.rs` ends that. Eliminating a phi means
    // writing ONE value from several places -- one per incoming edge -- and
    // where the incoming value is computed in the predecessor itself the
    // computation writes the phi's value directly. A loop counter that
    // starts at 0 therefore has a `const.i32 0` writing it in the preheader
    // and an `add` writing it on the back edge.
    //
    // Measured, and that is what this note is for: without the count below
    // `tests/018_while_sum.fi` returned 0 instead of 55, because every read
    // of the sum was replaced by the immediate `0` of its FIRST definition
    // and the `add` on the back edge wrote a value nobody looked at again.
    // The same trap sits in `codegen_a64.rs::layout`.
    let mut defs: HashMap<Val, u32> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                *defs.entry(d).or_insert(0) += 1;
            }
        }
    }
    let mut cand: HashMap<Val, i64> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                if defs.get(&d).copied().unwrap_or(0) != 1 {
                    continue;
                }
                let v = i.ty.truncate(*c);
                // Up to 32 bits the immediate may use the whole unsigned
                // range: `cmp $0xffffffff,%r9d` computes exactly right with
                // 32-bit operands. Without that, EOF (u32 0xFFFFFFFF) of all
                // things drops out of the immediates, and every EOF comparison
                // in the tokenizer loads its constant from a frame slot
                // (round 40: 52 such places in `tokenize` alone).
                let fits = if i.ty.bits() <= 32 {
                    v >= i32::MIN as i128 && v <= u32::MAX as i128
                } else {
                    v >= i32::MIN as i128 && v <= i32::MAX as i128
                };
                if !f.is_secret(d) && fits {
                    cand.insert(d, v as i64);
                }
            }
        }
    }
    if cand.is_empty() {
        return cand;
    }
    let mut bad: Vec<Val> = Vec::new();
    let kill = |v: Val, bad: &mut Vec<Val>| bad.push(v);
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                // Operand `a` goes through `load_ext` (movsx/movzx) -> no immediate.
                //
                // ROUND SPEED -- THE DIVISOR STAYS AN IMMEDIATE. It used to
                // be struck out here too, and that is what kept
                // `emit_div_const` from ever seeing a constant divisor:
                // `bench/firn/bytecount.fi` put its 251 in `r8` and divided
                // by the register, 16.7 million times. Nothing needs it
                // struck: the only place that reads the divisor is
                // `load_ext`, which turns an immediate into `mov rcx, 251`
                // exactly as it turns a register into `mov rcx, r8`.
                Op::Bin(BinOp::Div, a, _) | Op::Bin(BinOp::Rem, a, _) => {
                    kill(*a, &mut bad);
                }
                Op::Bin(BinOp::Shl, a, _) | Op::Bin(BinOp::Shr, a, _) => kill(*a, &mut bad),
                Op::Cast { src, .. } => kill(*src, &mut bad),
                // untouchable (SPEC §9.2): unchanged as at the base path
                Op::Select { cond, a, b: b2 } => {
                    kill(*cond, &mut bad);
                    kill(*a, &mut bad);
                    kill(*b2, &mut bad);
                }
                Op::Barrier { val } => kill(*val, &mut bad),
                Op::SecureZero { addr, size } => {
                    kill(*addr, &mut bad);
                    kill(*size, &mut bad);
                }
                _ => {}
            }
        }
        match &b.term {
            Term::BrCond { cond, .. } => kill(*cond, &mut bad),
            Term::Switch { val, .. } => kill(*val, &mut bad),
            _ => {}
        }
    }
    for v in bad {
        cand.remove(&v);
    }
    cand
}

/// `alloca`s whose address appears only as a `load`/`store` address or as the
/// base of a `ptradd`: they are addressed through `rbp` directly, the pointer
/// never has to sit in a register.
fn direct_frame_addrs(f: &Func, fr: &Frame) -> HashMap<Val, u64> {
    let mut cand: HashMap<Val, u64> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { .. }) = (i.dst, &i.op) {
                if let Some(Some(off)) = fr.alloca_off.get(d as usize).copied() {
                    if !f.is_secret(d) {
                        cand.insert(d, off);
                    }
                }
            }
        }
    }
    if cand.is_empty() {
        return cand;
    }
    let mut bad: Vec<Val> = Vec::new();
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                Op::Load { .. } => {}
                Op::Store { val, .. } => bad.push(*val),
                Op::PtrAdd { off, .. } => bad.push(*off),
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    bad.extend(buf.iter().copied());
                }
            }
        }
        match &b.term {
            Term::Ret(Some(v)) | Term::BrCond { cond: v, .. } | Term::Switch { val: v, .. } => {
                bad.push(*v)
            }
            _ => {}
        }
    }
    for v in bad {
        cand.remove(&v);
    }
    cand
}

// ------------------------------------------------------------- Linear scan ---

#[derive(Clone, Copy)]
struct Iv {
    val: Val,
    start: usize,
    end: usize,
    weight: u64,
    /// Pool registers that are destroyed while this value is alive
    /// (round 90; see `inst_clobbers`). A register may be handed to the
    /// value exactly when its bit is NOT in here.
    killed: RegMask,
    /// ROUND 90 — is this a promoted `alloca` cell? Only measured with, not
    /// acted on: see the note at the hand-out below.
    #[allow(dead_code)]
    is_cell: bool,
}

/// Loop depth per block (approximation: back edge u->v with v <= u spans
/// the blocks [v, u]).
fn loop_depth(f: &Func) -> Vec<u32> {
    let nb = f.blocks.len();
    let mut depth = vec![0u32; nb];
    for (u, b) in f.blocks.iter().enumerate() {
        for s in b.term.successors() {
            let v = s as usize;
            if v <= u && v < nb {
                for d in depth.iter_mut().take(u + 1).skip(v) {
                    *d += 1;
                }
            }
        }
    }
    for d in depth.iter_mut() {
        if *d > 4 {
            *d = 4;
        }
    }
    depth
}

/// The instruction ranges of the loops (round 90). A back edge `u -> v` with
/// `v <= u` makes every position from the start of block `v` to the end of
/// block `u` part of one loop.
///
/// It exists for the promoted cells. A cell holds a variable, and a variable
/// in a loop is read again AFTER the back edge: its register has to survive
/// the whole loop, not just the stretch between its first and its last
/// access. Until round 90 that was answered by giving every cell the
/// interval `[0, last access]` -- which covers every loop that starts after
/// the beginning of the function, and covers it at the price of ALSO
/// crossing everything that happens before the cell is ever touched.
///
/// In `bench/firn/matmul.fi` that was three calls to `alloc()` in the first
/// ten instructions of `main`. Nine cells, all of them "crossing a call"
/// because of those three, all nine competing for the five callee-saved
/// registers -- five of them lost and went to the stack, among them the
/// counters of the innermost loop. Measured with `FIRN_RA_STATS=1`:
/// `cellivs=9 cellslost=5`.
#[allow(dead_code)]
fn loop_ranges(f: &Func, block_start: &[usize], block_end: &[usize]) -> Vec<(usize, usize)> {
    let nb = f.blocks.len();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (u, b) in f.blocks.iter().enumerate() {
        for sbb in b.term.successors() {
            let v = sbb as usize;
            if v <= u && v < nb {
                out.push((block_start[v], block_end[u]));
            }
        }
    }
    out
}

/// Widens `[s, e]` until it contains every loop it touches, or gives up.
///
/// Monotone: `s` only falls, `e` only rises, both bounded by the function,
/// and a pass that changes nothing is the fixpoint. A pass that DOES change
/// something has absorbed at least one range, so `loops.len() + 1` passes
/// are enough -- and if that is somehow not enough, the answer is `None` and
/// the caller falls back to the interval that was always safe (`[0, last
/// access]`). Never a half widened range: that would be a wrong-code bug of
/// exactly the kind this round exists to stop making.
#[allow(dead_code)]
fn widen_to_loops(mut s: usize, mut e: usize, loops: &[(usize, usize)]) -> Option<(usize, usize)> {
    for _ in 0..=loops.len() {
        let mut changed = false;
        for &(ls, le) in loops {
            // ranges that OVERLAP the interval pull it out to their own ends
            if ls <= e && s <= le {
                if ls < s {
                    s = ls;
                    changed = true;
                }
                if le > e {
                    e = le;
                    changed = true;
                }
            }
        }
        if !changed {
            return Some((s, e));
        }
    }
    None
}

/// Carries out the complete allocation.
pub fn allocate(f: &Func) -> Alloc {
    let nv = f.val_types.len();
    let nb = f.blocks.len();
    let mut locs: Vec<Loc> = Vec::with_capacity(nv);
    let (frame, _) = layout(f, 0);
    for v in 0..nv {
        locs.push(Loc::Slot(frame.slot.get(v).copied().unwrap_or(0)));
    }
    let mut alloc = Alloc {
        locs,
        alias: HashMap::new(),
        alias_src: HashMap::new(),
        imms: HashMap::new(),
        frame_addr: HashMap::new(),
        cells: HashMap::new(),
        cell_ty: HashMap::new(),
        saved: Vec::new(),
        frame,
        stats: None,
    };
    // Safety net against an explosion of memory/time on huge functions:
    // then it stays with the (correct) stack model.
    if nb == 0 || nv == 0 || nv.saturating_mul(nb) > 8_000_000 {
        return alloc;
    }
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return alloc;
    }

    let live = compute_live(f);
    // ROUND 87: the cause distribution, only when it is asked for.
    let mut st = RaStats::default();
    let want_stats = std::env::var_os("FIRN_RA_STATS").is_some();
    // ROUND 87, STAGE 2 -- the exact crossings are no longer just a
    // measurement, they are the answer the allocator works with.
    //
    // Only when the data flow reached its fixed point: below the round limit
    // of `compute_live` the live sets may be TOO SMALL, and a value that is
    // wrongly thought not to survive a call would get `r10` and be destroyed
    // by that call. Then the conservative interval question stands again,
    // exactly as before this round. `FIRN_RA_ROUGH=1` forces that state for
    // troubleshooting.
    let exact = if live.converged && std::env::var_os("FIRN_RA_ROUGH").is_none() {
        Some(exact_crossings(f, &live))
    } else {
        None
    };
    let depth = loop_depth(f);
    let cells = promotable_cells(f);
    alloc.imms = immediate_consts(f);
    alloc.frame_addr = direct_frame_addrs(f, &alloc.frame);
    for v in alloc.imms.keys() {
        alloc.locs[*v as usize] = Loc::Slot(0);
    }

    // Where does something get destroyed, and what (round 90). ONE list, from
    // ONE table -- see `inst_clobbers`. The rough answer below sums the masks
    // over the interval, the exact one over the control flow; before round 90
    // these were two lists with two different op sets, and the difference was
    // the bug of this round.
    let mut clob_pos: Vec<(usize, RegMask)> = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            let m = inst_clobbers(i);
            if m != 0 {
                clob_pos.push((live.pos[bi][ii], m));
            }
        }
    }
    // The interval answer: everything destroyed anywhere between the first
    // and the last touch of the value. Always safe, never finer than the
    // control flow.
    let rough = |sp: usize, ep: usize| -> RegMask {
        let mut m: RegMask = 0;
        for &(p, k) in clob_pos.iter() {
            if sp <= p && p <= ep {
                m |= k;
            }
        }
        m
    };

    // intervals + weights
    let mut start = vec![usize::MAX; nv];
    let mut end = vec![0usize; nv];
    let mut weight = vec![0u64; nv];
    let touch = |v: Val, p: usize, w: u64, start: &mut Vec<usize>, end: &mut Vec<usize>, weight: &mut Vec<u64>| {
        let v = v as usize;
        if v >= nv {
            return;
        }
        if p < start[v] {
            start[v] = p;
        }
        if p > end[v] {
            end[v] = p;
        }
        weight[v] = weight[v].saturating_add(w);
    };
    for i in 0..f.params.len() {
        touch(i as Val, 0, 1, &mut start, &mut end, &mut weight);
    }
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        let w = 10u64.saturating_pow(depth[bi]);
        for v in 0..nv {
            if live.live_in[bi][v] {
                touch(v as Val, live.block_start[bi], 0, &mut start, &mut end, &mut weight);
            }
            if live.live_out[bi][v] {
                touch(v as Val, live.block_end[bi], 0, &mut start, &mut end, &mut weight);
            }
        }
        for (ii, i) in b.insts.iter().enumerate() {
            let p = live.pos[bi][ii];
            buf.clear();
            i.op.uses(&mut buf);
            let uses: Vec<Val> = buf.clone();
            for u in uses {
                touch(u, p, w, &mut start, &mut end, &mut weight);
            }
            if let Some(d) = i.dst {
                touch(d, p, w, &mut start, &mut end, &mut weight);
            }
        }
        let tv = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = tv {
            touch(v, live.block_end[bi], w, &mut start, &mut end, &mut weight);
        }
    }

    let mut ivs: Vec<Iv> = Vec::new();
    for v in 0..nv {
        if start[v] == usize::MAX {
            st.no_interval += 1;
            continue;
        }
        if f.is_secret(v as Val) {
            st.secret += 1;
            continue; // secret values stay at the stack slot (SPEC §9.2)
        }
        if cells.contains_key(&(v as Val)) {
            continue; // is treated as a cell
        }
        if alloc.imms.contains_key(&(v as Val)) || alloc.frame_addr.contains_key(&(v as Val)) {
            continue; // needs no place at all
        }
        // Values whose place IS the memory (alloca addresses) may get a
        // register; their content keeps lying in the frame.
        let (s, e) = (start[v], end[v]);
        // The exact answer where it exists, the interval answer otherwise.
        // Both are safe; the exact one is only narrower.
        let killed = match &exact {
            Some(x) => x[v],
            None => rough(s, e),
        };
        if want_stats {
            st.ivs += 1;
            if rough(s, e) & M_CALL != 0 {
                st.cross_call += 1;
            }
            if killed & M_CALL != 0 {
                st.cross_call_exact += 1;
            }
        }
        ivs.push(Iv { val: v as Val, start: s, end: e, weight: weight[v], killed, is_cell: false });
    }
    for (&c, _) in cells.iter() {
        let cv = c as usize;
        if start[cv] == usize::MAX {
            continue;
        }
        // The cell has to sit in the register from the start of the function
        // to the last access (its content survives blocks without access).
        //
        // ROUND 90 TRIED THE TIGHTER ANSWER and threw it away again. A cell
        // does not live before its first access, and a variable in a loop is
        // read again after the back edge, so `[first access, last access]`
        // widened over the enclosing loops (`loop_ranges`/`widen_to_loops`
        // above, still there and still used by nothing else) is both correct
        // and much shorter: in `bench/firn/matmul.fi` it took `main` from
        // four promoted cells to six and from five lost to three, because
        // the three `alloc()` calls in the first ten instructions stopped
        // counting as "crossed" for counters that appear much later.
        //
        // It did not pay. Measured over eight benchmarks at `release-fast`
        // (`tools/bench90/icount.py`, instructions really executed):
        // statemachine -4.6 %, bytecount -0.9 %, matmul +5.8 %, everything
        // else identical to the instruction -- total -0.16 %. More cells in
        // registers, the same amount of work, and one clear loser. Round 90
        // is a round about a wrong-code bug; a register allocation change
        // that buys nothing is exactly the kind of thing that should not
        // ride along with it, so `release-fast` comes out of this round
        // BIT-IDENTICAL to what went in.
        let s = 0usize;
        let e = end[cv];
        let killed = rough(s, e);
        // Cells are almost always the hottest values: double the weight.
        // (Round 87: they are counted apart. A cell has to survive from the
        // start of the function to its last access, so `crosses_call` is
        // almost always true for it and always CORRECTLY so -- counting it
        // in with the value intervals would make the false-positive rate
        // look better than it is.)
        st.cell_ivs += 1;
        ivs.push(Iv {
            val: c,
            start: s,
            end: e,
            weight: weight[cv].saturating_mul(2).max(1),
            killed,
            is_cell: true,
        });
    }
    ivs.sort_by_key(|i| (i.start, i.end, i.val));

    // ROUND 87: the real register pressure. NOT `active.len()` -- `active`
    // only holds the intervals that got a register, so it can never exceed
    // twelve and would report "pressure 12" for a function that needs 900.
    // This is a sweep over the interval ends: how many intervals overlap at
    // the worst position of the function.
    if want_stats {
        let mut ev: Vec<(usize, i32)> = Vec::with_capacity(ivs.len() * 2);
        for i in ivs.iter() {
            ev.push((i.start, 1));
            ev.push((i.end + 1, -1));
        }
        ev.sort_unstable();
        let mut cur = 0i32;
        for (_, d) in ev {
            cur += d;
            if cur as usize > st.max_live {
                st.max_live = cur as usize;
            }
        }
    }

    // ---- the linear scan itself ------
    //
    // Four pools, from the most restricted to the freest register. `fits`
    // checks whether a register tolerates the crossings of an interval.
    fn fits(iv: &Iv, r: &str) -> bool {
        let b = reg_bit(r);
        b != 0 && iv.killed & b == 0
    }
    let mut free_saved: Vec<&'static str> = CALLEE_SAVED.to_vec();
    let mut free_temp: Vec<&'static str> = TEMP_REGS.to_vec();
    let mut free_arg: Vec<&'static str> = ARG_SPARE.to_vec();
    let mut free_div: Vec<&'static str> = DIV_SPARE.to_vec();
    let mut active: Vec<(Iv, &'static str)> = Vec::new();
    let mut assign: HashMap<Val, &'static str> = HashMap::new();
    let mut used_saved: Vec<&'static str> = Vec::new();

    let free = |r: &'static str,
                     free_saved: &mut Vec<&'static str>,
                     free_temp: &mut Vec<&'static str>,
                     free_arg: &mut Vec<&'static str>,
                     free_div: &mut Vec<&'static str>| {
        if TEMP_REGS.contains(&r) {
            free_temp.push(r);
        } else if ARG_SPARE.contains(&r) {
            free_arg.push(r);
        } else if DIV_SPARE.contains(&r) {
            free_div.push(r);
        } else {
            free_saved.push(r);
        }
    };

    for iv in ivs.iter().copied() {
        // Release intervals that have expired.
        //
        // ROUND 49 — `<` RATHER THAN `<=`, A SOUNDNESS BUG.
        //
        // The intervals are CLOSED: `crosses_call` checks `s <= p && p <= e`.
        // Two closed intervals [a,b] and [c,d] with a <= c therefore overlap
        // exactly when c <= b — and then they must NOT be given the same
        // register.
        //
        // With `<=` an interval ending at p was released as soon as the next
        // one BEGAN at p. In a classic linear scan that is allowed, because
        // there "end" is the last USE and "start" is the DEFINITION of the
        // same instruction (read first, then write). Here that assumption
        // does not hold: the interval bounds come from `live_in`/
        // `live_out` at BLOCK BOUNDARIES as well. A value that lives from a block
        // placed later on across an earlier one thus gets the block start as
        // its beginning — and shared the register with a value defined
        // exactly there.
        //
        // MEASURED on tests/820_gc_finalizer.fi (`release-fast` only, so with
        // register allocation only): `%355 = z + 24` had the interval
        // [175,356], `%135 = call gc_collect()` the interval [175,175].
        // Both got `r12`; at run time the path bb45 (definition of %355) ->
        // bb46 -> bb21 (`mov r12, rax`) -> ... -> bb49 (`mov r8, [r12]`) ran,
        // and the program died with a memory access fault.
        //
        // The bug is OLDER than round 49: six lines of dummy code in
        // `gc_collect` suffice to trigger it with the compiler of the base
        // (cc1710f). Round 49 merely ran into it. With `<` the allocation is
        // minimally tighter; the measurement is in docs/ROUND49.md §3.
        let mut k = 0;
        while k < active.len() {
            if active[k].0.end < iv.start {
                let (_, r) = active.remove(k);
                free(r, &mut free_saved, &mut free_temp, &mut free_arg, &mut free_div);
            } else {
                k += 1;
            }
        }
        // fill the restricted pools first, callee-saved last (it costs
        // prologue/epilogue) — unless the interval crosses a call, in which
        // case only callee-saved ones come into question.
        // `rposition` keeps the old `pop()` preference (the last free
        // register of a pool) wherever the whole pool fits, and skips exactly
        // the registers this interval may not have.
        let take = |pool: &mut Vec<&'static str>| -> Option<&'static str> {
            pool.iter().rposition(|r| fits(&iv, r)).map(|k| pool.remove(k))
        };
        // MEASURED, round 90: letting a cell ask the callee-saved pool FIRST
        // (it lives long, and the four temp registers are what the short
        // lived values around it have) reads well and does nothing --
        // statemachine 691.2 -> 699.6 million instructions, matmul unchanged
        // (tools/bench90/icount.py). So the order stays one order for
        // everybody: the cheap registers first, the ones that cost a push
        // and a pop last.
        let pick = take(&mut free_temp)
            .or_else(|| take(&mut free_div))
            .or_else(|| take(&mut free_arg))
            .or_else(|| take(&mut free_saved));
        match pick {
            Some(r) => {
                if CALLEE_SAVED.contains(&r) && !used_saved.contains(&r) {
                    used_saved.push(r);
                }
                assign.insert(iv.val, r);
                active.push((iv, r));
            }
            None => {
                // Spilling: the active interval with the SMALLEST weight
                // (uses x loop depth) clears the register. At equal weight
                // the later end decides.
                let mut worst: Option<usize> = None;
                for (k, (a, r)) in active.iter().enumerate() {
                    if !fits(&iv, r) {
                        continue; // this register does not help us
                    }
                    let better = match worst {
                        None => true,
                        Some(w) => (a.weight, usize::MAX - a.end)
                            < (active[w].0.weight, usize::MAX - active[w].0.end),
                    };
                    if better {
                        worst = Some(k);
                    }
                }
                if let Some(w) = worst {
                    if active[w].0.weight < iv.weight {
                        let (old, r) = active.remove(w);
                        assign.remove(&old.val);
                        assign.insert(iv.val, r);
                        active.push((iv, r));
                        st.evicted += 1;
                        continue;
                    }
                }
                // otherwise this value stays in the stack slot
                if iv.killed & M_CALL != 0 {
                    st.lost_call += 1;
                } else {
                    st.lost_plain += 1;
                }
            }
        }
    }

    if want_stats {
        for c in cells.keys() {
            if !assign.contains_key(c) {
                st.cells_lost += 1;
            }
        }
        alloc.stats = Some(st);
    }
    // enter the result
    for (v, r) in assign.iter() {
        if cells.contains_key(v) {
            alloc.cells.insert(*v, r);
            if let Some(t) = cells.get(v) {
                alloc.cell_ty.insert(*v, *t);
            }
        } else {
            alloc.locs[*v as usize] = Loc::Reg(r);
        }
    }
    let read = count_reads(f);
    let mut nbuf = Vec::new();
    if std::env::var("FIRN_NO_ALIAS").is_err() {
    // ---- cell alias (round 40) -----------------------------------------
    // `d = load c` with exactly ONE use in the same block, before which the
    // cell is not written: d needs no place of its own, its value is already
    // in the cell register. The load disappears during emission, the use
    // reads the cell register directly through `loc()` — that strikes three
    // quarters of the `mov r9, r15` copies in front of every use of the
    // loop counter (hottest loop of `decode`: 3 copies per iteration,
    // 33.5 M iterations in the realweb run).
    for b in &f.blocks {
        for (ii, inst) in b.insts.iter().enumerate() {
            let (addr, d) = match (&inst.op, inst.dst) {
                (Op::Load { addr }, Some(d)) => (*addr, d),
                _ => continue,
            };
            // FULL WIDTH ONLY: at 8/16/32 bits the load pulls the relevant bits
            // out via movzx/mov32 — the cell register contains leftovers in the
            // upper part, and an alias would read them along (round 40, failure
            // picture 211_generic_struct/430_ct_select/416_error_output).
            if inst.ty.bits().max(8) != 64 {
                continue;
            }
            let rc = match alloc.cells.get(&addr) {
                Some(r) => *r,
                None => continue,
            };
            let needs = read.get(d as usize).copied().unwrap_or(0) as usize;
            if needs == 0 {
                continue;
            }
            // ALL uses must lie in this block before the cell is written
            // again (with several uses the alias strikes every copy
            // nonetheless, say the counter as index for source AND target
            // in the copy loop of the decoder).
            let mut found = 0usize;
            let mut ok = false;
            // ROUND 41: the cell register must not be written by ANY other
            // value between the load and the last use. The allocator did not
            // know about the lifetime extended by the alias and was allowed
            // to hand `rc` to a value whose span does not overlap that of the
            // cell value — then something foreign is in it when reading.
            // Failure picture: `43 - start` in bin/print.fi (`print_binop`)
            // turned into `43 - &tab[start]`, because `lea` wrote the address
            // into exactly that register; the length ran below zero and
            // `buf_grow` spun forever (endless loop in .astdump on every
            // `||`).
            let mut destroys = false;
            for nj in b.insts.iter().skip(ii + 1) {
                nbuf.clear();
                nj.op.uses(&mut nbuf);
                found += nbuf.iter().filter(|u| **u == d).count();
                if let Some(d2) = nj.dst {
                    if d2 != d && used_register(&alloc, d2, rc) {
                        destroys = true;
                        break;
                    }
                }
                // A call destroys all caller-saved registers; the cell
                // value then sits only in the frame, not in the
                // register. (Failure picture: bin/layoutdump.fi crashed
                // in `intern_find` with t=0.)
                if matches!(nj.op, Op::Call { .. } | Op::CallIndirect { .. } | Op::Syscall { .. } | Op::ThreadSpawn { .. })
                    && !CALLEE_SAVED.contains(&rc)
                {
                    destroys = true;
                    break;
                }
                if matches!(nj.op, Op::Store { addr: a2, .. } if a2 == addr) {
                    break; // after that the value loaded is stale
                }
            }
            if destroys {
                continue;
            }
            if found == needs {
                ok = true;
            } else {
                // A remaining use can sit in the terminator (brcond/ret).
                // Switch NOT: that one expects the value in the frame
                // (codegen_switch).
                let in_term = match &b.term {
                    Term::BrCond { cond, .. } if *cond == d => 1,
                    Term::Ret(Some(v)) if *v == d => 1,
                    _ => 0,
                };
                ok = found + in_term == needs && in_term > 0;
            }
            if ok {
                alloc.alias.insert(d, rc);
                alloc.alias_src.insert(d, addr);
            }
        }
    }

    }
    if std::env::var("FIRN_NO_INPLACE").is_err() {
    // ---- cell update on the spot (round 40) -----------------------------
    // `d1 = load c` (alias), `v = d1 + k`, `store c, v` with one use each:
    // v gets the cell register as its place — the emission then computes
    // directly in the cell register (`lea r15, [r15+1]`) and the store
    // disappears. Condition: between the definition of v and the store the
    // cell is neither read nor written (otherwise a reader in between would
    // see the new value too early) and no call separates the two. No ALIAS
    // value of the same cell may still be outstanding in that window either
    // (its use expects the old content, which would already have been
    // overwritten).
    for b in &f.blocks {
        for (ii, inst) in b.insts.iter().enumerate() {
            let (a, k, v) = match (&inst.op, inst.dst) {
                (Op::Bin(BinOp::Add | BinOp::Sub, a, k), Some(v)) => (*a, *k, v),
                _ => continue,
            };
            let (rc, cell) = match (alloc.alias.get(&a), alloc.alias_src.get(&a)) {
                (Some(r), Some(z)) => (*r, *z),
                _ => continue,
            };
            if alloc.imm(k).is_none()
                || read.get(v as usize).copied().unwrap_or(0) != 1
                || f.is_secret(v)
            {
                continue;
            }
            let mut ok = false;
            for (jj, nj) in b.insts.iter().enumerate().skip(ii + 1) {
                match &nj.op {
                    Op::Store { addr: a2, val } if *a2 == cell => {
                        ok = *val == v;
                        break;
                    }
                    Op::Load { addr: a2 } if *a2 == cell => break,
                    Op::Call { .. }
                    | Op::CallIndirect { .. }
                    | Op::Syscall { .. }
                    | Op::ThreadSpawn { .. } => break,
                    _ => {}
                }
                // is the use of an alias value of the same cell still
                // outstanding? That one expects the OLD content.
                nbuf.clear();
                nj.op.uses(&mut nbuf);
                if nbuf.iter().any(|u| {
                    *u != a && alloc.alias_src.get(u) == Some(&cell)
                }) {
                    break;
                }
                let _ = jj;
            }
            if ok {
                alloc.locs[v as usize] = Loc::Reg(rc);
            }
        }
    }

    }

    // Frame including save slots for the callee-saved registers in use
    used_saved.sort_unstable();
    let (frame, slots) = layout(f, used_saved.len() as u64);
    alloc.frame = frame;
    alloc.saved = used_saved.iter().copied().zip(slots.iter().map(|(_, o)| *o)).collect();
    alloc
}

// ------------------------------------------------------------- Emission ---

/// Register name at the wanted width.
fn rn(name: &str, bits: u32) -> String {
    let b = match bits {
        8 => 0,
        16 => 1,
        32 => 2,
        _ => 3,
    };
    let tab: [[&str; 4]; 15] = [
        ["al", "ax", "eax", "rax"],
        ["cl", "cx", "ecx", "rcx"],
        ["dl", "dx", "edx", "rdx"],
        ["bl", "bx", "ebx", "rbx"],
        ["sil", "si", "esi", "rsi"],
        ["dil", "di", "edi", "rdi"],
        ["r8b", "r8w", "r8d", "r8"],
        ["r9b", "r9w", "r9d", "r9"],
        ["r10b", "r10w", "r10d", "r10"],
        ["r11b", "r11w", "r11d", "r11"],
        ["r12b", "r12w", "r12d", "r12"],
        ["r13b", "r13w", "r13d", "r13"],
        ["r14b", "r14w", "r14d", "r14"],
        ["r15b", "r15w", "r15d", "r15"],
        ["bpl", "bp", "ebp", "rbp"],
    ];
    let row = match name {
        "rax" => 0,
        "rcx" => 1,
        "rdx" => 2,
        "rbx" => 3,
        "rsi" => 4,
        "rdi" => 5,
        "r8" => 6,
        "r9" => 7,
        "r10" => 8,
        "r11" => 9,
        "r12" => 10,
        "r13" => 11,
        "r14" => 12,
        "r15" => 13,
        _ => 14,
    };
    tab[row][b].to_string()
}

struct Ra<'a> {
    f: &'a Func,
    a: &'a Alloc,
    /// How often is every value read as an operand? Needed for fusing `cmp`
    /// and the conditional jump: only when the comparison result is read
    /// EXACTLY ONCE (namely by the terminator) may the `setcc` be left
    /// out.
    read: Vec<u32>,
    /// Addresses whose only use is the memory access following IMMEDIATELY —
    /// they move entirely into its operand.
    offset: HashMap<Val, Address>,
    /// Instructions (scaling `shl`/`mul`) that disappear entirely along the way.
    skipped: std::collections::HashSet<Val>,
    /// Instructions of which only the FILLING of their register is left:
    /// value -> source value. The rest of the computation sits in the memory
    /// operand of the following access.
    preloader: HashMap<Val, Val>,
}

/// A memory operand that the processor computes itself:
/// `[base + index*factor + offset]` (x86-64 SIB addressing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Address {
    base: &'static str,
    /// index register with factor 1, 2, 4 or 8
    index: Option<(&'static str, i64)>,
    offset: i64,
}

impl Address {
    fn text(&self) -> String {
        let mut s = String::from("[");
        s.push_str(self.base);
        if let Some((r, f)) = self.index {
            s.push('+');
            s.push_str(r);
            if f != 1 {
                s.push('*');
                s.push_str(&f.to_string());
            }
        }
        if self.offset > 0 {
            s.push('+');
            s.push_str(&self.offset.to_string());
        } else if self.offset < 0 {
            s.push('-');
            s.push_str(&(-self.offset).to_string());
        }
        s.push(']');
        s
    }
}

/// Address computations that may move completely into the memory access.
///
/// Up to round 43 this produced
///     lea r9, [r8+168]
///     mov r9, qword ptr [r9]
/// although x86-64 can do the offset itself:
///     mov r9, qword ptr [r8+168]
///
/// **Round 51** adds the other two parts of the x86 addressing — index
/// register and factor. Measured in the tokenizer (realweb, instruction
/// exact callgrind), before this round the picture was:
///
/// | pattern                                  |          Ir |  share |
/// |------------------------------------------|------------:|-------:|
/// | `shl k` + `lea (b,i,1)` + access         |  28.840.310 |  3,93 % |
/// | `lea (b,i,1)` + access                   |  16.231.553 |  2,21 % |
/// | `lea off(b)` + access                    |  14.432.184 |  1,97 % |
///
/// So what used to be
///     mov  r8, qword ptr [rbp-416]
///     shl  r8, 2
///     lea  r8, [r9+r8]
///     mov  r8d, dword ptr [r8]
/// now becomes
///     mov  r8, qword ptr [rbp-416]
///     mov  r8d, dword ptr [r9+r8*4]
///
/// **The conditions are deliberately tight**, because every loosening
/// extends the lifetime of the base — exactly the class that produced the
/// miscompile in round 40/41 (docs/ROUND41.md). Folding happens only when
///
///  * the address forming instruction is a `ptradd` or a **64-bit** `add`
///    (at 32 bits the addressing would NOT cut the overflow off),
///  * its result is read EXACTLY ONCE (terminators counted along),
///  * this one reader is the instruction following IMMEDIATELY in the
///    same block and is a `load`/`store` over exactly this address,
///  * base (and index, if any) lie in a register and are neither a frame
///    address nor a promoted cell nor a cell alias,
///  * for the factor: the scaling is a **64-bit** `shl` by 0..3 or a
///    `mul` by 1/2/4/8, sits IMMEDIATELY in front of the address forming
///    instruction, and its result is likewise read exactly once,
///  * no value of the chain is `secret` (SPEC §9.2: no data dependent
///    access).
///
/// The moment at which base and index are read thus shifts by exactly the
/// one or two instructions that **disappear entirely** in the process —
/// after that nothing lies between them any more, in particular no `call`.
/// The only registers written at the new place are the target of the access
/// (which reads its address first — `mov r8d, dword ptr [r9+r8*4]` is
/// correct) and the home of the skipped instructions, which is not written
/// at all any more.
///
/// Yields `(addresses per value, scalings skipped)`.
/// Can be switched off with FIRN_NO_FALTUNG=1 (troubleshooting).
fn foldable_addresses(
    f: &Func,
    a: &Alloc,
    read: &[u32],
) -> (HashMap<Val, Address>, std::collections::HashSet<Val>, HashMap<Val, Val>) {
    use std::collections::HashSet;
    let mut out: HashMap<Val, Address> = HashMap::new();
    let mut away: HashSet<Val> = HashSet::new();
    let mut before: HashMap<Val, Val> = HashMap::new();
    if std::env::var_os("FIRN_NO_FALTUNG").is_some() {
        return (out, away, before);
    }
    // Does the value simply lie in a register — without special handling?
    let pure_reg = |v: Val| -> Option<&'static str> {
        if a.imm(v).is_some() || a.cell(v).is_some() || f.is_secret(v) {
            return None;
        }
        if a.alias.contains_key(&v) || a.frame_addr.contains_key(&v) {
            return None;
        }
        match a.place(v) {
            Loc::Reg(r) => Some(r),
            Loc::Slot(_) => None,
        }
    };
    for b in &f.blocks {
        for (idx, i) in b.insts.iter().enumerate() {
            let d = match i.dst {
                Some(d) => d,
                None => continue,
            };
            // (1) address forming instruction
            let (base, off) = match &i.op {
                Op::PtrAdd { base, off } => (*base, *off),
                // An `add` forms an address only when it computes at full
                // width. At 32 bits FIR cuts the result off, the addressing
                // would not do that.
                Op::Bin(BinOp::Add, x, y) if i.ty.bits() == 64 => (*x, *y),
                _ => continue,
            };
            if read.get(d as usize).copied() != Some(1) || f.is_secret(d) {
                continue;
            }
            if a.alias.contains_key(&d) || a.frame_addr.contains_key(&d) || a.cell(d).is_some() {
                continue;
            }
            // (2) the ONE reader is the access following immediately
            let n = match b.insts.get(idx + 1) {
                Some(n) => n,
                None => continue,
            };
            let fits = match &n.op {
                Op::Load { addr } => *addr == d,
                Op::Store { addr, val } => *addr == d && *val != d,
                _ => false,
            };
            if !fits {
                continue;
            }
            // Registers that the following access still has to READ itself —
            // they must not serve as a preload target.
            let value_reg: Option<&'static str> = match &n.op {
                Op::Store { val, .. } => match a.place(*val) {
                    Loc::Reg(r) => Some(r),
                    _ => None,
                },
                _ => None,
            };
            // The base either lies in a register already — or it is loaded
            // into the register of the address computation, which would
            // otherwise stay unused (case C, round 51):
            //     mov rax, qword ptr [rbp-8]      rather than   mov rax, [rbp-8]
            //     mov r9, qword ptr [rax+8]                     lea r9, [rax+8]
            //                                                   mov r9, [r9]
            let base_may_read = |v: Val| -> bool {
                !f.is_secret(v)
                    && a.cell(v).is_none()
                    && !a.alias.contains_key(&v)
                    && !a.frame_addr.contains_key(&v)
                    && a.imm(v).is_none()
            };
            let (br, base_preload) = match pure_reg(base) {
                Some(r) => (r, false),
                None => match (a.loc(d), base_may_read(base)) {
                    (Loc::Reg(dr), true) if Some(dr) != value_reg => (dr, true),
                    _ => continue,
                },
            };
            // (3a) constant offset
            if let Some(k) = a.imm(off) {
                if (0..=i32::MAX as i64).contains(&k) && !f.is_secret(off) {
                    out.insert(d, Address { base: br, index: None, offset: k });
                    if base_preload {
                        before.insert(d, base);
                    }
                }
                continue;
            }
            // (3b) index with factor: the scaling sits right in front of it
            if read.get(off as usize).copied() == Some(1) && idx > 0 {
                let p = &b.insts[idx - 1];
                let skal = if p.dst == Some(off) && p.ty.bits() == 64 {
                    match &p.op {
                        Op::Bin(BinOp::Shl, xi, ki) => match a.imm(*ki) {
                            Some(k) if (0..=3).contains(&k) => Some((*xi, 1i64 << k)),
                            _ => None,
                        },
                        Op::Bin(BinOp::Mul, xi, ki) => match a.imm(*ki) {
                            Some(k) if [1, 2, 4, 8].contains(&k) => Some((*xi, k)),
                            _ => None,
                        },
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some((xi, fact)) = skal {
                    if !f.is_secret(off) && !a.alias.contains_key(&off) && a.cell(off).is_none() {
                        // Case A: the index lies in a register itself —
                        // the scaling disappears with no replacement.
                        if let Some(ir) = pure_reg(xi) {
                            if !base_preload || ir != br {
                                out.insert(
                                    d,
                                    Address { base: br, index: Some((ir, fact)), offset: 0 },
                                );
                                away.insert(off);
                                if base_preload {
                                    before.insert(d, base);
                                }
                                continue;
                            }
                        }
                        // Case B: the index lies in the frame, but the
                        // scaling has a register home. Then the UNSCALED
                        // value is loaded there and the factor is left to
                        // the addressing — one instruction instead of
                        // two.
                        if let (Loc::Reg(ir), true) = (a.loc(off), base_may_read(xi)) {
                            if ir != br && Some(ir) != value_reg {
                                out.insert(
                                    d,
                                    Address { base: br, index: Some((ir, fact)), offset: 0 },
                                );
                                before.insert(off, xi);
                                if base_preload {
                                    before.insert(d, base);
                                }
                                continue;
                            }
                        }
                    }
                }
            }
            // (3c) index directly from a register (factor 1)
            if let Some(ir) = pure_reg(off) {
                if !base_preload || ir != br {
                    out.insert(d, Address { base: br, index: Some((ir, 1)), offset: 0 });
                    if base_preload {
                        before.insert(d, base);
                    }
                }
            }
        }
    }
    (out, away, before)
}

/// Counts per value how often it appears as an operand (instructions, terminators).
fn count_reads(f: &Func) -> Vec<u32> {
    let mut n = vec![0u32; f.val_types.len()];
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for v in buf.iter() {
                if let Some(c) = n.get_mut(*v as usize) {
                    *c += 1;
                }
            }
        }
        match &b.term {
            Term::Ret(Some(v)) | Term::BrCond { cond: v, .. } | Term::Switch { val: v, .. } => {
                if let Some(c) = n.get_mut(*v as usize) {
                    *c += 1;
                }
            }
            _ => {}
        }
    }
    n
}

impl<'a> Ra<'a> {
    /// Operand of a value at full width.
    fn opnd(&self, v: Val) -> String {
        if let Some(k) = self.a.imm(v) {
            return format!("{}", k);
        }
        match self.a.place(v) {
            Loc::Reg(r) => r.to_string(),
            Loc::Slot(off) => format!("qword ptr [rbp-{}]", off),
        }
    }
    /// Operand of a value at the width `bits`.
    fn opnd_w(&self, v: Val, bits: u32) -> String {
        if let Some(k) = self.a.imm(v) {
            return format!("{}", k);
        }
        match self.a.place(v) {
            Loc::Reg(r) => rn(r, bits),
            Loc::Slot(off) => format!("{} [rbp-{}]", size_word(bits), off),
        }
    }
    /// Load a value completely into a scratch register.
    fn load_full(&self, e: &mut Emitter, r: &str, v: Val) {
        let o = self.opnd(v);
        if o != r {
            e.line(&format!("mov {}, {}", r, o));
        }
    }
    /// Load a value sign/zero extended to `to_bits` into a scratch register.
    fn load_ext(&self, e: &mut Emitter, r: &str, v: Val, ty: FTy, to_bits: u32) {
        if let Some(k) = self.a.imm(v) {
            // The immediate constant is already trimmed to the right type.
            e.line(&format!("mov {}, {}", rn(r, to_bits.max(32)), k));
            return;
        }
        let bits = ty.bits().max(8);
        if bits >= to_bits {
            let o = self.opnd_w(v, to_bits);
            let d = rn(r, to_bits);
            if o != d {
                e.line(&format!("mov {}, {}", d, o));
            }
            return;
        }
        let src = self.opnd_w(v, bits);
        match (ty.signed(), bits) {
            (true, _) if bits == 32 => {
                e.line(&format!("movsxd {}, {}", rn(r, to_bits), src))
            }
            (true, _) => e.line(&format!("movsx {}, {}", rn(r, to_bits), src)),
            (false, b) if b == 32 => e.line(&format!("mov {}, {}", rn(r, 32), src)),
            (false, _) => e.line(&format!("movzx {}, {}", rn(r, to_bits.min(32)), src)),
        }
    }
    /// Write a scratch register into the target value.
    fn store_dst(&self, e: &mut Emitter, d: Val, r: &str) {
        match self.a.loc(d) {
            Loc::Reg(dr) => {
                if dr != r {
                    e.line(&format!("mov {}, {}", dr, r));
                }
            }
            Loc::Slot(off) => e.line(&format!("mov qword ptr [rbp-{}], {}", off, r)),
        }
    }
}

/// Register aware emission of a function.
/// `None` = this path is not responsible, the base path takes over.
pub(crate) fn emit_func_ra(e: &mut Emitter, f: &Func) -> Option<Result<(), String>> {
    if !supported(f) {
        return None;
    }
    let a = allocate(f);
    // ROUND 82 (`FIRN_RA_STATS=1`): how good IS this allocation? One line per
    // function: how many values it has, how many of them got a register, how
    // many stayed on the stack, and how many `alloca` cells were promoted.
    // Everything else in this round measured throughput; this measures the
    // allocator itself, and `tools/bench82/ra_report.py` adds it up over a
    // whole library.
    if std::env::var_os("FIRN_RA_STATS").is_some() {
        let nv = f.val_types.len();
        let regs = a.locs.iter().filter(|l| matches!(l, Loc::Reg(_))).count();
        let imm = a.imms.len();
        let addr = a.frame_addr.len();
        // Values that neither sit in a register nor are an immediate nor a
        // frame address: those are the ones that really cost a memory access.
        let spilled = nv.saturating_sub(regs + imm + addr);
        eprintln!(
            "RA {} values={} regs={} imm={} frameaddr={} spilled={} cells={} insts={}",
            f.name, nv, regs, imm, addr, spilled, a.cells.len(), f.inst_count()
        );
        // ROUND 87: and WHY. One line more per function, same format.
        if let Some(t) = a.stats {
            eprintln!(
                "RA-WHY {} ivs={} crosscall={} crosscall_exact={} noiv={} secret={}                  lostcall={} lostplain={} evicted={} cellslost={} cellivs={} maxlive={}",
                f.name, t.ivs, t.cross_call, t.cross_call_exact, t.no_interval,
                t.secret, t.lost_call, t.lost_plain, t.evicted, t.cells_lost,
                t.cell_ivs, t.max_live
            );
        }
    }
    // The function is emitted into a buffer of its own first; after that the
    // register descriptor post pass strikes spill stores with an immediate
    // reload of the same value (445x statically in the tokenizer run, round 37).
    let mut tmp = Emitter::default();
    match emit_with(&mut tmp, f, &a) {
        Ok(()) => {
            // ROUND 90: the panic arms of the checked operations, behind the
            // function they belong to. They go through the descriptor pass
            // with the rest -- every one of them starts with a `.L` label,
            // which resets the descriptor, so they can believe nothing.
            tmp.flush_cold();
            let nv = f.val_types.len();
            e.out.push_str(&descriptor_peephole(&tmp.out, nv));
            Some(Ok(()))
        }
        Err(err) => Some(Err(err)),
    }
}

// ------------------------------------------------- Register descriptor ---
//
// Post pass over the finished assembler of ONE function. The allocation
// writes values without a register into their stack slot (`store_dst`) and
// loads them again at the next use (`load_full`) — but if the value is still
// standing unchanged in the register it was stored from, the reload is for
// nothing: either entirely (same register) or as a memory access (other
// target register: `mov rB, rA` instead of `mov rB, [rbp-X]`).
//
// Tracked are **value slots** exclusively: their offsets lie at `8..=nv*8`
// (layout() hands them out first). `alloca` places and save slots lie
// behind those and are never tracked — writing through pointers
// (`Op::Store`/`CopyMem`/`SecureZero`) can hit them, value slots on the
// other hand never (their address does not exist in the program).
//
// Invalidation (conservative, safety before gain):
//  * block boundaries (label) and backward/jump lines reset the state —
//    the following block can arrive from elsewhere with a foreign state.
//  * `call` clears all caller-saved registers out of the descriptor,
//    `syscall` rax/rcx/r11, `rep movsb/stosb` rdi/rsi/rcx, `div/idiv`
//    rax/rdx, `cqo/cdq` rdx, `setcc` al (= rax).
//  * ROUND 90: the ONE OPERAND `mul`/`imul` clear rax and rdx. They write
//    `rdx:rax` while naming neither register, so the old allowlist below
//    saw `mul rcx` and invalidated NOTHING -- the same blind spot in the
//    same file that made the allocator hand `rdx` to a live value
//    (`inst_clobbers` above). `cpuid` (rax/rbx/rcx/rdx), the `lock`
//    prefixed read-modify-writes and `crc32` had the same hole.
//  * Every other instruction that writes a tracked register as its target
//    operand (mov/lea/add/.../cmov) invalidates exactly that register.
//  * ROUND 90, AND THIS IS THE PART THAT MATTERS: anything NOT recognised
//    here throws the whole descriptor away instead of being ignored. The
//    old code fell through silently, so every instruction somebody adds to
//    the backend without touching this table was a latent wrong-code bug.
//    Now the worst a new instruction can cost is a missed reload.
fn descriptor_peephole(asm: &str, nv: usize) -> String {
    /// 64-bit trunk register of a register name at any width.
    fn stem(r: &str) -> &str {
        match r {
            "al" | "ax" | "eax" | "rax" => "rax",
            "bl" | "bx" | "ebx" | "rbx" => "rbx",
            "cl" | "cx" | "ecx" | "rcx" => "rcx",
            "dl" | "dx" | "edx" | "rdx" => "rdx",
            "sil" | "si" | "esi" | "rsi" => "rsi",
            "dil" | "di" | "edi" | "rdi" => "rdi",
            "bpl" | "bp" | "ebp" | "rbp" => "rbp",
            _ => {
                let b = r.as_bytes();
                if b.len() >= 3 && b[0] == b'r' && matches!(b[b.len() - 1], b'd' | b'w' | b'b')
                    && r[1..r.len() - 1].chars().all(|c| c.is_ascii_digit())
                {
                    &r[..r.len() - 1]
                } else {
                    r
                }
            }
        }
    }
    /// Bit width of a register name.
    fn width_of(r: &str) -> u32 {
        match r {
            "al" | "bl" | "cl" | "dl" | "sil" | "dil" | "bpl" => 8,
            "ax" | "bx" | "cx" | "dx" | "si" | "di" | "bp" => 16,
            "eax" | "ebx" | "ecx" | "edx" | "esi" | "edi" | "ebp" => 32,
            _ => {
                let b = r.as_bytes();
                if b.len() >= 3 && b[0] == b'r' && r[1..r.len() - 1].chars().all(|c| c.is_ascii_digit())
                {
                    match b[b.len() - 1] {
                        b'b' => 8,
                        b'w' => 16,
                        b'd' => 32,
                        _ => 64,
                    }
                } else {
                    64
                }
            }
        }
    }
    let max_slot = nv as u64 * 8;
    let mut out = String::with_capacity(asm.len());
    // slot_off -> (register with the same content, width of the storage)
    let mut sync: HashMap<u64, (String, u32)> = HashMap::new();
    // register -> slot_off (the reverse)
    let mut holds: HashMap<String, u64> = HashMap::new();
    // ZERO EXTENSION (round 51). `nullab[r] = k` means: all bits from k on
    // are guaranteed zero in `r`. Without an entry nothing is known.
    //
    // The ground for this is a property of x86-64 that holds throughout the
    // post pass: EVERY write to a 32-bit register zeroes the upper 32 bits
    // of the 64-bit register. A `movzx r32, byte ptr [..]` even says that
    // everything from bit 8 on is zero.
    //
    // Only with that may a narrow reload be struck: `mov [X], r8d` followed
    // by `mov r8d, [X]` loads back exactly the bits that are already in r8
    // — but only if r8 is zero up top anyway. Exactly that condition was
    // missing in round 43, which is why the case was deferred there
    // (docs/ROUND43.md §6).
    let mut nullab: HashMap<String, u32> = HashMap::new();
    let kill_reg = |r: &str,
                    sync: &mut HashMap<u64, (String, u32)>,
                    holds: &mut HashMap<String, u64>| {
        if let Some(off) = holds.remove(r) {
            if sync.get(&off).map(|s| s.0.as_str()) == Some(r) {
                sync.remove(&off);
            }
        }
    };
    for line in asm.lines() {
        let t = line.trim_start();
        if !line.starts_with("    ") || t.is_empty() {
            // Labels, directives, comments at the start of a line. A label is
            // a block boundary: the state of the predecessor does not hold.
            if t.ends_with(':') && !t.starts_with('.') {
                sync.clear();
                holds.clear();
                nullab.clear();
            } else if t.starts_with(".L") && t.ends_with(':') {
                sync.clear();
                holds.clear();
                nullab.clear();
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let mut parts = t.splitn(2, ' ');
        let mn = parts.next().unwrap_or("");
        let ops = parts.next().unwrap_or("").trim();
        // Target forms that we track/replace.
        // Storing into a VALUE slot, at EVERY width (round 51: formerly
        // `qword` only). `off <= max_slot` narrows it down to the value
        // slots — `alloca` places lie behind those and can be written
        // through pointers.
        let st_width = if t.starts_with("mov qword ptr [rbp-") {
            Some(64)
        } else if t.starts_with("mov dword ptr [rbp-") {
            Some(32)
        } else if t.starts_with("mov word ptr [rbp-") {
            Some(16)
        } else if t.starts_with("mov byte ptr [rbp-") {
            Some(8)
        } else {
            None
        };
        if let Some(bw) = st_width {
            let rest = &t[t.find("[rbp-").unwrap() + 5..];
            if let Some(kl) = rest.find(']') {
                let off: u64 = rest[..kl].parse().unwrap_or(0);
                let q = rest[kl + 1..].trim_start_matches(',').trim();
                if off >= 8 && off <= max_slot && width_of(q) == bw && is_reg64(stem(q)) {
                    let q = stem(q).to_string();
                    kill_reg(&q, &mut sync, &mut holds);
                    sync.insert(off, (q.clone(), bw));
                    holds.insert(q, off);
                    out.push_str(line);
                    out.push('\n');
                    continue;
                }
                // Immediate constant or the like: the slot has a new content
                // that is in no register.
                if off >= 8 && off <= max_slot {
                    if let Some((r, _)) = sync.remove(&off) {
                        holds.remove(&r);
                    }
                }
            }
        }
        // Reload from a value slot. Four forms, each with its own
        // condition:
        //   mov  rY,  qword ptr [X]   needs storage width 64
        //   mov  rYd, dword ptr [X]   needs >= 32 and nullab[rY] <= 32
        //   movzx rYd, byte ptr [X]   needs >=  8 and nullab[rY] <=  8
        //   movzx rYd, word ptr [X]   needs >= 16 and nullab[rY] <= 16
        // `movsx`/`movsxd` stay outside: sign extension cannot be shown
        // from `nullab`.
        let rlform = if mn == "mov" {
            if let Some(k) = ops.find(", qword ptr [rbp-") {
                Some((k, 17usize, 64u32, 64u32))
            } else {
                ops.find(", dword ptr [rbp-").map(|k| (k, 17usize, 32u32, 32u32))
            }
        } else if mn == "movzx" {
            if let Some(k) = ops.find(", byte ptr [rbp-") {
                Some((k, 16usize, 8u32, 8u32))
            } else {
                ops.find(", word ptr [rbp-").map(|k| (k, 16usize, 16u32, 16u32))
            }
        } else {
            None
        };
        if let Some((kl, before, min, ndbits)) = rlform {
            let target = &ops[..kl];
            let zb = width_of(target);
            // The target width must fit the load form: qword -> 64, else 32.
            let fits_target = if min == 64 { zb == 64 } else { zb == 32 };
            if fits_target && is_reg64(stem(target)) {
                if let Some(end) = ops[kl + before..].find(']') {
                    let off: u64 = ops[kl + before..kl + before + end].parse().unwrap_or(0);
                    if off >= 8 && off <= max_slot {
                        let z = stem(target).to_string();
                        let hit = match sync.get(&off) {
                            Some((r2, bw)) if *bw >= min => Some(r2.clone()),
                            _ => None,
                        };
                        if let Some(r2) = hit {
                            let same = r2 == z;
                            let already_null = nullab.get(&z).copied().unwrap_or(64) <= ndbits;
                            if same && (min == 64 || already_null) {
                                // The value is already in the register exactly like that.
                                kill_reg(&z, &mut sync, &mut holds);
                                sync.insert(off, (z.clone(), min));
                                holds.insert(z.clone(), off);
                                if min < 64 {
                                    nullab.insert(z, ndbits);
                                }
                                continue;
                            }
                            if !same && min == 64 {
                                out.push_str(&format!("    mov {}, {}\n", z, r2));
                                kill_reg(&z, &mut sync, &mut holds);
                                sync.insert(off, (z.clone(), 64));
                                holds.insert(z.clone(), off);
                                nullab.remove(&z);
                                continue;
                            }
                        }
                        kill_reg(&z, &mut sync, &mut holds);
                        sync.insert(off, (z.clone(), min));
                        holds.insert(z.clone(), off);
                        if min < 64 {
                            nullab.insert(z, ndbits);
                        } else {
                            nullab.remove(&z);
                        }
                        out.push_str(line);
                        out.push('\n');
                        continue;
                    }
                }
            }
        }
        // Jumps/return: the state of the following block is unknown.
        if mn.starts_with('j') || mn == "ret" {
            sync.clear();
            holds.clear();
            nullab.clear();
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "call" {
            for r in ["rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"] {
                kill_reg(r, &mut sync, &mut holds);
                nullab.remove(r);
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "syscall" {
            for r in ["rax", "rcx", "r11"] {
                kill_reg(r, &mut sync, &mut holds);
                nullab.remove(r);
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "rep" {
            for r in ["rdi", "rsi", "rcx"] {
                kill_reg(r, &mut sync, &mut holds);
                nullab.remove(r);
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "div" || mn == "idiv" {
            kill_reg("rax", &mut sync, &mut holds);
            kill_reg("rdx", &mut sync, &mut holds);
            nullab.remove("rax");
            nullab.remove("rdx");
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "cqo" || mn == "cdq" {
            kill_reg("rdx", &mut sync, &mut holds);
            nullab.remove("rdx");
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn.starts_with("set") {
            kill_reg("rax", &mut sync, &mut holds); // the target is always `al` at the RA path
            nullab.remove("rax"); // `setcc al` leaves the upper bits standing
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // ROUND 90 -- the ONE OPERAND forms of `mul`/`imul`: the product is
        // `rdx:rax` (`dx:ax`, `high:low`) and neither register is written
        // down. `mul cl` alone stays inside `ax`, but clearing rdx as well
        // costs nothing and needs no width case.
        if mn == "mul" || (mn == "imul" && !ops.contains(',')) {
            kill_reg("rax", &mut sync, &mut holds);
            kill_reg("rdx", &mut sync, &mut holds);
            nullab.remove("rax");
            nullab.remove("rdx");
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if mn == "cpuid" {
            for r in ["rax", "rbx", "rcx", "rdx"] {
                kill_reg(r, &mut sync, &mut holds);
                nullab.remove(r);
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // `lock xadd [rcx], rax` / `lock cmpxchg [rcx], rdx`: a register AND
        // a memory cell at an address the descriptor cannot follow.
        if mn == "lock" {
            sync.clear();
            holds.clear();
            nullab.clear();
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // Instructions that write their first operand register.
        if matches!(
            mn,
            "mov" | "movzx" | "movsx" | "movsxd" | "lea" | "add" | "sub" | "and" | "or" | "xor"
                | "imul" | "shl" | "sar" | "shr" | "neg" | "not" | "pop" | "crc32" | "adc"
                | "sbb" | "bswap" | "rol" | "ror" | "bsr" | "bsf" | "popcnt" | "tzcnt" | "lzcnt"
        ) || mn.starts_with("cmov")
        {
            let target = ops.split(',').next().unwrap_or("").trim();
            // CAUTION: the target name can be narrow (`xor eax, eax` zeroes
            // all of rax) — the check has to go for the TRUNK REGISTER,
            // otherwise a stale descriptor entry survives (round 40: made
            // tests/305_dtoa_hardcases compute wrong).
            let z = stem(target);
            if !target.contains('[') && is_reg64(z) {
                let zs = z.to_string();
                kill_reg(&zs, &mut sync, &mut holds);
                // Carry the zero extension forward (round 51). A write to a
                // 32-bit register zeroes the upper 32 bits; `movzx` from an
                // 8/16-bit source says even more. Everything else makes the
                // content up top unknown.
                let bw = width_of(target);
                if mn == "movzx" {
                    let q = ops.rsplit(',').next().unwrap_or("").trim();
                    let of = if q.starts_with("byte ptr") {
                        8
                    } else if q.starts_with("word ptr") {
                        16
                    } else {
                        width_of(q)
                    };
                    if bw >= 32 && (of == 8 || of == 16) {
                        nullab.insert(zs, of);
                    } else {
                        nullab.remove(&zs);
                    }
                } else if bw == 32 {
                    nullab.insert(zs, 32);
                } else {
                    nullab.remove(&zs);
                }
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // ROUND 90 -- THE CATCH-ALL. Reaching here means the mnemonic is not
        // in any table above. Read-only instructions are listed by name;
        // everything else is unknown, and an unknown instruction may write
        // anything, so the descriptor is emptied rather than kept.
        if !matches!(mn, "cmp" | "test" | "push" | "cld" | "std" | "nop" | "ud2" | "int3" | "hlt")
            && !mn.starts_with('#')
            && !mn.starts_with('.')
        {
            sync.clear();
            holds.clear();
            nullab.clear();
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Is VARIABLE information being produced (`--no-opt`, see `dwarf.rs`)?
/// Then the base path takes over: the debugger is told that every local sits
/// at a fixed frame offset, and that is true only while nothing lives in a
/// register.
///
/// ROUND 94: this used to ask for debug LINES, which now exist at every
/// build level (`fir::Loc`) -- asking that question would have sent every
/// optimized function down the base path and thrown the register allocation
/// away. Lines and variables are two different promises, and only the second
/// one needs the frame.
fn debug_vars_active(_f: &Func) -> bool {
    crate::dwarf::with_variables()
}

fn supported(f: &Func) -> bool {
    let basic = unsupported_basic(f);
    if let Some(g) = basic {
        if std::env::var_os("FIRN_RA_WARN").is_some()
            || std::env::var_os("FIRN_RA_STATS").is_some()
        {
            eprintln!("RA-BASE {} reason={} insts={}", f.name, g, f.inst_count());
        }
        return false;
    }
    true
}

/// Why does `f` fall back to the base path? `None` = allocation possible.
fn unsupported_basic(f: &Func) -> Option<String> {
    // ROUND 52: `#[interrupt]` has a calling convention of its own (rescue all
    // registers, `iretq`). It sits on the base path of `codegen_x86.rs`.
    if f.interrupt {
        return Some("#[interrupt]".into());
    }
    if debug_vars_active(f) {
        return Some("variable debug information active".into());
    }
    // FLOATING POINT: this allocator knows only the integer registers. `f64`
    // lives in the SSE registers and needs a second register class with
    // intervals of its own. As long as that is missing, a function containing
    // `f64` goes over the base path in `codegen_x86.rs` — correct, but without
    // register allocation. Stated honestly in SPEC §14.1.f64.
    // ROUND 71: `f32` too. The linear scan knows only the integer registers;
    // as long as that is so, EVERY function with floating point in it goes
    // through the base path (SPEC 14.1.f64, restriction F1).
    if f.val_types.iter().any(|t| t.is_float()) {
        return Some("f64 in the value set".into());
    }
    // ROUND 82: `v128` is a second register class of its own (xmm) with
    // sixteen byte slots. The linear scan hands out integer registers only —
    // a function with a vector value therefore goes over the base path of
    // `codegen_x86.rs`, which has the xmm value cache of `simd.rs`.
    if f.val_types.iter().any(|t| *t == FTy::V128) || f.params.iter().any(|t| *t == FTy::V128)
        || f.ret == FTy::V128
    {
        return Some("v128 in the value set".into());
    }
    if f.blocks.is_empty() {
        return Some("no blocks".into());
    }
    if let Some((i, b)) = f.blocks.iter().enumerate().find(|(i, b)| b.id as usize != *i) {
        return Some(format!("block numbers not consecutive (index {}, id {})", i, b.id));
    }
    for b in &f.blocks {
        if matches!(b.term, Term::Unset) {
            return Some(format!("block {} without terminator", b.id));
        }
        for i in &b.insts {
            match &i.op {
                Op::Call { .. }
                | Op::CallIndirect { .. }
                | Op::VtabAddr { .. }
                | Op::FnRef { .. }
                | Op::GlobalAddr { .. } => {}
                Op::Syscall { args } => {
                    if args.is_empty() || args.len() > 7 {
                        return Some(format!("syscall with {} arguments", args.len()));
                    }
                }
                // ROUND 52: inline assembler and MMIO go over the base
                // path. Both bind fixed registers and are `volatile`; the
                // allocation would have needed a special rule for that,
                // and a special rule in the allocator is exactly the sort
                // of code that produced the bug of round 40. Kernel code
                // therefore runs without register allocation — slower, but
                // provably right. Stated honestly in docs/ROUND52.md.
                Op::Asm { .. } => return Some("Inline-Assembler".into()),
                // ROUND 82: of the vector instructions exactly the three with
                // a SCALAR result get through here (`crc32`, `cpu_features`) —
                // they touch no xmm register and need no cache. Everything
                // that produces or consumes a `v128` goes over the base path;
                // `f.val_types` has already caught that above, but an
                // instruction whose result is scalar while an operand is a
                // vector would slip through, so it is named here too.
                Op::Simd { kind, .. } => {
                    if !matches!(
                        kind,
                        crate::simd::SimdKind::Crc32U8
                            | crate::simd::SimdKind::Crc32U64
                            | crate::simd::SimdKind::CpuFeatures
                    ) {
                        return Some("vector instruction".into());
                    }
                }
                Op::MmioLoad { .. } | Op::MmioStore { .. } => {
                    return Some("MMIO access".into())
                }
                _ => {}
            }
        }
    }
    None
}

fn emit_with(e: &mut Emitter, f: &Func, a: &Alloc) -> Result<(), String> {
    let read = count_reads(f);
    let (offset, skipped, preloader) = foldable_addresses(f, a, &read);
    let ra = Ra { f, a, read, offset, skipped, preloader };
    // ROUND 72: one label counter per function (panic_rt.rs::SiteCounter).
    let mut site = crate::panic_rt::SiteCounter::new(&f.name);
    e.raw("");
    // Linker symbol through the one spot (codegen_x86::label -> modules::symbol)
    e.raw(&format!(".globl {}", label(&f.name)));
    e.raw(&format!("{}:", label(&f.name)));
    // Line of the `fn` declaration for the debugger (dwarf.rs).
    e.forget_loc();
    if let Some((file, line)) = crate::dwarf::fn_line(&f.name) {
        e.loc_at(crate::fir::Loc { file, line, col: 0 });
    }
    e.line("push rbp");
    e.line("mov rbp, rsp");
    if a.frame.size > 0 {
        e.line(&format!("sub rsp, {}", a.frame.size));
    }
    for (r, off) in &a.saved {
        e.line(&format!("mov qword ptr [rbp-{}], {}", off, r));
    }
    // Bring the parameters out of the argument registers into their home.
    // CAUTION: `r8`/`r9` are argument registers 5/6 AND at the same time
    // possible homes of earlier parameters. That is why all slot targets come
    // first (they overwrite no register), then the register targets IN PARALLEL.
    let mut prolog_moves: Vec<(String, String)> = Vec::new();
    for (i, _t) in f.params.iter().enumerate().take(ARG_REGS.len()) {
        match ra.a.loc(i as Val) {
            Loc::Slot(off) => {
                e.line(&format!("mov qword ptr [rbp-{}], {}", off, ARG_REGS[i]))
            }
            Loc::Reg(dst) => prolog_moves.push((dst.to_string(), ARG_REGS[i].to_string())),
        }
    }
    parallel_reg_moves(e, &prolog_moves);
    // Parameters from the seventh on lie in the frame of the CALLER (System V:
    // [rbp+16], [rbp+24], … — in front of those sit the saved return address
    // and the saved rbp). They are fetched ONLY AFTER the parallel moves: their
    // target register could otherwise overwrite a source that is still needed.
    // `rax`, being a scratch register, is never the home of a value and may
    // serve as intermediate storage here.
    for (i, _t) in f.params.iter().enumerate().skip(ARG_REGS.len()) {
        let of = 16 + 8 * (i - ARG_REGS.len()) as u64;
        match ra.a.loc(i as Val) {
            Loc::Slot(off) => {
                e.line(&format!("mov rax, qword ptr [rbp+{}]", of));
                e.line(&format!("mov qword ptr [rbp-{}], rax", off));
            }
            Loc::Reg(dst) => e.line(&format!("mov {}, qword ptr [rbp+{}]", dst, of)),
        }
    }
    // Round 51: the blocks are no longer printed in their FIR order but
    // along traces (see `emit_order`).
    let order = emit_order(f);
    // ROUND SPEED -- a loop head starts at a 16 byte boundary.
    //
    // Round 3 moved blocks around, and two benchmarks whose emitted
    // instructions did not change by a single byte moved with them --
    // `sieve` and `bubblesort` got SLOWER although their hot loops got one
    // taken branch cheaper. What changed was the ADDRESS: a loop whose body
    // straddles a fetch window costs an extra window every iteration, and
    // where it straddles is decided by how much code stands in front of it.
    //
    // `rustc` writes `.p2align 4` in front of every hot loop for exactly
    // this reason. The third argument caps the padding: if reaching the
    // boundary would cost more than ten bytes, the alignment is skipped
    // rather than pushing ten nops into the instruction cache.
    let aligned = loop_entry_flags(f);
    for (k, &bi) in order.iter().enumerate() {
        let b = &f.blocks[bi];
        if aligned[bi] {
            e.raw("    .p2align 4, 0x90");
        }
        e.raw(&format!("{}:", block_label(&f.name, b.id)));
        // Fallthrough: if the jump target sits right behind it, the jump
        // disappears (saves one `jmp` per BrCond with else==next block).
        let next = order.get(k + 1).map(|&j| f.blocks[j].id);
        emit_block(e, &ra, b, next, &mut site)?;
    }
    Ok(())
}

/// **Block layout along traces** (round 51).
///
/// So far the blocks were printed in their FIR numbering. Wherever neither
/// `then` nor `else` happened to be the next block, another `jmp` stood
/// behind the conditional jump — in the tokenizer at 641 places, measured at
/// **28.414.304 of 775.569.867 instructions (3,66 %)** for these
/// unconditional jumps alone:
///
/// ```text
/// cmp  -0x18(%rbp),%r8
/// jae  40dbd4          ; then
/// jmp  40dbe0          ; else — could have been a fallthrough
/// ```
///
/// The method is the usual greedy trace building: from `bb0` the preferred
/// successor is followed as long as it is still free; once the trace breaks
/// off, it carries on at the smallest block not yet placed. Preferred is the
/// `else` branch — `emit_block` turns the condition around itself if `then`
/// follows instead, so no case gets lost.
///
/// **Why this cannot break anything.** The order concerns the OUTPUT
/// exclusively. Every block has one explicit terminator, and `emit_block`
/// leaves a jump out only when its target really does follow immediately
/// (`next`). Liveness analysis, intervals and register choice keep working
/// on the FIR order and are not touched here — a value still lies in the
/// same place for its whole lifetime, exactly as it did before this
/// change.
///
/// Can be switched off with `FIRN_NO_LAYOUT=1` (troubleshooting).
fn emit_order(f: &Func) -> Vec<usize> {
    let n = f.blocks.len();
    if std::env::var_os("FIRN_NO_LAYOUT").is_some() {
        return (0..n).collect();
    }
    // ROUND SPEED -- THE LOOP BODY IS THE HOT SIDE, NOT THE EXIT.
    //
    // The trace used to prefer `else_bb` unconditionally. At a loop head
    // `brcond i < n, body, exit` the `else` side is the EXIT -- the one edge
    // of the whole loop that is taken exactly once. So the exit fell through
    // and the body was placed somewhere else, which cost the loop TWO taken
    // jumps per iteration:
    //
    //     head: cmp r12, 240
    //           jb   body        <- taken, every pass
    //     exit: ...              <- fallthrough, reached once
    //     body: ...
    //           jmp  head        <- taken, every pass
    //
    // With the loop depth as the tiebreaker the body falls through and the
    // conditional jump becomes the not-taken one:
    //
    //     head: cmp r12, 240
    //           jae  exit        <- NOT taken, every pass
    //     body: ...
    //           jmp  head        <- taken
    //
    // `emit_block` inverts the condition itself when `then` is the next
    // block, so no case is lost. Layout only -- liveness, intervals and
    // register choice all keep working on the FIR order.
    let depth = layout_depth(f);
    let mut placed = vec![false; n];
    let mut out: Vec<usize> = Vec::with_capacity(n);
    let mut free = 0usize;
    let mut b = 0usize;
    while out.len() < n {
        // Lay a trace as long as the preferred successor is still free.
        loop {
            placed[b] = true;
            out.push(b);
            let w = match &f.blocks[b].term {
                Term::Br(t) => Some(*t as usize),
                Term::BrCond { then_bb, else_bb, .. } => {
                    let el = *else_bb as usize;
                    let th = *then_bb as usize;
                    let el_free = el < n && !placed[el];
                    let th_free = th < n && !placed[th];
                    // The deeper nested successor wins; on a tie the old
                    // preference for `else` stands.
                    let take_then = th_free
                        && (!el_free
                            || depth.get(th).copied().unwrap_or(0)
                                > depth.get(el).copied().unwrap_or(0));
                    if take_then {
                        Some(th)
                    } else if el_free {
                        Some(el)
                    } else {
                        Some(th)
                    }
                }
                Term::Switch { default, .. } => Some(*default as usize),
                Term::Ret(_) | Term::Unset => None,
            };
            match w {
                Some(t) if t < n && !placed[t] => b = t,
                _ => break,
            }
        }
        // ROUND SPEED -- A LOOP IS LAID OUT IN ONE PIECE.
        //
        // When the trace breaks, the old rule carried on at the lowest
        // block not yet placed. That interleaves regions that have nothing
        // to do with each other. `bench/firn/bubblesort.fi`: the `if x > y`
        // arm of the innermost loop ended up 0xa1 octets past the loop,
        // BEHIND the whole final summing loop, and the conditional jump to
        // it grew from a two octet `jg rel8` to a six octet `jg rel32` --
        // inside the hot loop, and the swap it jumps to is taken on a good
        // half of the comparisons.
        //
        // Carrying on with the DEEPEST unplaced block instead keeps the
        // rest of the loop next to the loop. Ties go to the lowest number,
        // so sibling loops of the same depth stay in their old order.
        while free < n && placed[free] {
            free += 1;
        }
        if free >= n {
            break;
        }
        let mut best = free;
        for i in (free + 1)..n {
            if !placed[i] && depth[i] > depth[best] {
                best = i;
            }
        }
        b = best;
    }
    out
}

/// ROUND SPEED -- loop depth per block, over the NATURAL LOOPS.
///
/// `loop_depth` above answers the same question with an approximation: a
/// back edge `u -> v` with `v <= u` counts the block NUMBERS `v..=u` one
/// deeper. That is right whenever a loop body is numbered contiguously and
/// wrong the moment it is not -- and `lower.rs` numbers an `if` inside a
/// loop AFTER the block that follows the loop. `bench/firn/bytecount.fi`:
///
///     bb7:  brcond k < n, bb15, bb8      <- the loop head
///     bb8:  ...                          <- the EXIT, but inside the outer loop
///     bb14: br bb7                       <- the latch
///     bb15: ...                          <- the body, numbered past the latch
///
/// The back edge is `bb14 -> bb7`, so the approximation counts `bb7..=bb14`
/// and gives the body `bb15` depth 0 while the exit `bb8` gets 2 (it lies
/// inside the outer pass loop as well). The layout of round 2 then picked
/// the EXIT as the fallthrough -- exactly the case it was written to avoid,
/// on the hottest loop of that benchmark.
///
/// This one asks the definition instead: a back edge is an edge `b -> h`
/// whose target dominates its source, and the natural loop belonging to it
/// is `h` plus everything that reaches `b` without passing `h`
/// (`licm.rs` uses the same one). Every block of that body counts one
/// deeper.
///
/// It is used by the block layout alone -- a wrong number here can cost a
/// jump, never correctness. The existing `loop_depth` stays untouched: the
/// register allocator reads it, and changing what the allocator sees is a
/// different round from changing where the blocks are printed.
fn layout_depth(f: &Func) -> Vec<u32> {
    let n = f.blocks.len();
    let mut d = vec![0u32; n];
    if n < 2 || n > 1024 {
        return d;
    }
    // The same cheap pre-check `licm.rs` uses: if every edge goes strictly
    // forward the graph is acyclic and there is no natural loop to find.
    let any_backward = f
        .blocks
        .iter()
        .enumerate()
        .any(|(b, blk)| blk.term.successors().into_iter().any(|s| (s as usize) <= b));
    if !any_backward {
        return d;
    }
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return d;
    }
    let preds = crate::mem2reg::preds(f);
    let dom = crate::mem2reg::dominators(f);
    let mut body = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    for b in 0..n {
        for s in f.blocks[b].term.successors() {
            let h = s as usize;
            if h >= n || !dom[b][h] {
                continue;
            }
            body.iter_mut().for_each(|x| *x = false);
            body[h] = true;
            stack.clear();
            if b != h {
                body[b] = true;
                stack.push(b);
            }
            while let Some(x) = stack.pop() {
                for &p in &preds[x] {
                    if !body[p] {
                        body[p] = true;
                        stack.push(p);
                    }
                }
            }
            for (i, inside) in body.iter().enumerate() {
                if *inside {
                    d[i] += 1;
                }
            }
        }
    }
    d
}

/// ROUND SPEED -- which blocks get a `.p2align 4` in front of them.
///
/// The target of a BACKWARD edge in the block numbering. That is a superset
/// of the real loop heads (a numbering that is not in reverse post order can
/// have a backward edge without a loop), and a superset is exactly right
/// here: aligning a block that is not a loop head wastes at most ten bytes,
/// missing one costs a fetch window per iteration. No dominator matrix
/// needed for that.
///
/// On with `FIRN_ALIGN_LOOPS=1` -- see `docs/ROUNDSPEED.md`, round 4:
/// measured, it wins on one benchmark and loses on another.
fn loop_entry_flags(f: &Func) -> Vec<bool> {
    let n = f.blocks.len();
    let mut out = vec![false; n];
    if std::env::var_os("FIRN_ALIGN_LOOPS").is_none() {
        return out;
    }
    for (b, blk) in f.blocks.iter().enumerate() {
        for s in blk.term.successors() {
            let h = s as usize;
            if h < n && h <= b {
                out[h] = true;
            }
        }
    }
    out
}

/// ROUND SPEED -- the magic number for an UNSIGNED division by `d`.
///
/// Hacker's Delight figure 10-3, written for `n` bits instead of 32.
/// Yields `(m, add, s)`, to be used as
///
/// ```text
///   q = mulhi(x, m) >> s                       (add == false)
///   q = (((x - mulhi(x, m)) >> 1) + mulhi) >> (s - 1)   (add == true)
/// ```
///
/// The `add` case exists because the exact magic number needs `n + 1` bits
/// for some divisors; `m` is then the low `n` bits of it and the fixup adds
/// the missing power of two back without ever overflowing.
///
/// Checked against Python's integer division for both widths over every
/// divisor from 2 to 300 plus the edges, and for `d = 251` at 64 bits it
/// yields 367465021388636487 -- the same constant `rustc` puts into
/// `bench/rust/bytecount.rs`.
fn magic_u(d: u128, n: u32) -> (u128, bool, u32) {
    debug_assert!(n >= 8 && n <= 64 && d >= 2 && d < (1u128 << n));
    let mask: u128 = (1u128 << n) - 1;
    let half: u128 = 1u128 << (n - 1);
    let mut a = false;
    let neg_d = ((1u128 << n) - d) & mask;
    let nc = mask - (neg_d % d);
    let mut p = n - 1;
    let two_p = 1u128 << p;
    let mut q1 = two_p / nc;
    let mut r1 = two_p - q1 * nc;
    let mut q2 = (two_p - 1) / d;
    let mut r2 = (two_p - 1) - q2 * d;
    loop {
        p += 1;
        if r1 >= nc - r1 {
            q1 = 2 * q1 + 1;
            r1 = 2 * r1 - nc;
        } else {
            q1 *= 2;
            r1 *= 2;
        }
        if r2 + 1 >= d - r2 {
            if q2 >= half - 1 {
                a = true;
            }
            q2 = 2 * q2 + 1;
            r2 = 2 * r2 + 1 - d;
        } else {
            if q2 >= half {
                a = true;
            }
            q2 *= 2;
            r2 = 2 * r2 + 1;
        }
        let delta = d - 1 - r2;
        if !(p < 2 * n && (q1 < delta || (q1 == delta && r1 == 0))) {
            break;
        }
    }
    ((q2 + 1) & mask, a, p - n)
}

/// ROUND SPEED -- the magic number for a SIGNED division by `d`
/// (Hacker's Delight figure 10-1, generalised to `n` bits). Yields
/// `(m, s)`; `m` is the two's complement bit pattern of an `n` bit signed
/// number. Requires `|d| >= 2` and `|d| < 2^(n-1)`.
fn magic_s(d: i128, n: u32) -> (u128, u32) {
    debug_assert!(n >= 8 && n <= 64);
    let mask: u128 = (1u128 << n) - 1;
    let two: i128 = 1i128 << (n - 1);
    let ad: i128 = d.abs();
    let t: i128 = two + i128::from(d < 0);
    let anc = t - 1 - t % ad;
    let mut p = n - 1;
    let mut q1 = two / anc;
    let mut r1 = two - q1 * anc;
    let mut q2 = two / ad;
    let mut r2 = two - q2 * ad;
    loop {
        p += 1;
        q1 *= 2;
        r1 *= 2;
        if r1 >= anc {
            q1 += 1;
            r1 -= anc;
        }
        q2 *= 2;
        r2 *= 2;
        if r2 >= ad {
            q2 += 1;
            r2 -= ad;
        }
        let delta = ad - r2;
        if !(q1 < delta || (q1 == delta && r1 == 0)) {
            break;
        }
    }
    let m = if d < 0 { -(q2 + 1) } else { q2 + 1 };
    ((m as u128) & mask, p - n)
}

/// `n` bit two's complement read as a signed number.
fn as_signed(v: u128, n: u32) -> i128 {
    let m = 1u128 << (n - 1);
    if v & m != 0 {
        (v as i128) - (1i128 << n)
    } else {
        v as i128
    }
}

/// ROUND SPEED -- `a / k` and `a % k` for a CONSTANT `k`, without `div`.
///
/// `div r64` costs some 14 to 47 cycles on this processor and blocks the
/// divider while it runs; a multiplication costs three. `bench/firn/
/// bytecount.fi` fills its 16 MiB buffer with `i % 251` -- 16.7 million
/// divisions in one loop, and `rustc` does not emit a single `div` for it.
///
/// Yields `false` when this path does not apply; the caller then emits the
/// `div` it always did. Refused: `k == 0` (the program is supposed to trap),
/// `k == 1` / `k == -1` (nothing to compute, and `-1` is the `MIN / -1`
/// special case) and anything wider than 64 bits.
///
/// Registers: `rax`, `rcx` and `rdx` only, exactly like the `div` path, so
/// `inst_clobbers` (which says `M_RDX` for both `Bin(Div|Rem)` and
/// `CheckedDiv`) stays right. The dividend is fetched into `rcx` ONCE, at
/// the top, and `mul`/`imul` leave `rcx` alone -- so nothing is ever read
/// from its home again after `rdx` has been overwritten.
fn emit_div_const(
    e: &mut Emitter,
    ra: &Ra,
    op: BinOp,
    ty: FTy,
    a: Val,
    k: i64,
    d: Val,
) -> bool {
    if ty.is_float() || ty.bits() > 64 {
        return false;
    }
    let bits = if ty.bits() > 32 { 64 } else { 32 };
    let rem = op == BinOp::Rem;
    let ra_w = |r: &str| rn(r, bits);
    if ty.signed() {
        let k = k as i128;
        if k == 0 || k == 1 || k == -1 || k.abs() >= (1i128 << (bits - 1)) {
            return false;
        }
        let (m, sh) = magic_s(k, bits);
        let ms = as_signed(m, bits);
        ra.load_ext(e, "rcx", a, ty, bits);
        e.line(&format!("mov {}, {}", ra_w("rax"), m));
        e.line(&format!("imul {}", ra_w("rcx")));
        if k > 0 && ms < 0 {
            e.line(&format!("add {}, {}", ra_w("rdx"), ra_w("rcx")));
        } else if k < 0 && ms > 0 {
            e.line(&format!("sub {}, {}", ra_w("rdx"), ra_w("rcx")));
        }
        if sh > 0 {
            e.line(&format!("sar {}, {}", ra_w("rdx"), sh));
        }
        // + 1 if the quotient came out negative (round towards zero)
        e.line(&format!("mov {}, {}", ra_w("rax"), ra_w("rdx")));
        e.line(&format!("shr {}, {}", ra_w("rax"), bits - 1));
        e.line(&format!("add {}, {}", ra_w("rdx"), ra_w("rax")));
        if rem {
            e.line(&format!("mov {}, {}", ra_w("rax"), ra_w("rcx")));
            e.line(&format!("imul {}, {}, {}", ra_w("rdx"), ra_w("rdx"), k as i64));
            e.line(&format!("sub {}, {}", ra_w("rax"), ra_w("rdx")));
            ra.store_dst(e, d, "rax");
        } else {
            ra.store_dst(e, d, "rdx");
        }
        return true;
    }
    // unsigned
    let ku = (k as u64) as u128 & ((1u128 << bits) - 1);
    if ku < 2 {
        return false;
    }
    let (m, add, sh) = magic_u(ku, bits);
    ra.load_ext(e, "rcx", a, ty, bits);
    e.line(&format!("mov {}, {}", ra_w("rax"), m));
    e.line(&format!("mul {}", ra_w("rcx")));
    // rdx = high word, rcx = the dividend, still untouched
    if rem {
        e.line(&format!("mov {}, {}", ra_w("rax"), ra_w("rcx")));
    }
    let q = if add {
        e.line(&format!("sub {}, {}", ra_w("rcx"), ra_w("rdx")));
        e.line(&format!("shr {}, 1", ra_w("rcx")));
        e.line(&format!("add {}, {}", ra_w("rcx"), ra_w("rdx")));
        if sh > 1 {
            e.line(&format!("shr {}, {}", ra_w("rcx"), sh - 1));
        }
        "rcx"
    } else {
        if sh > 0 {
            e.line(&format!("shr {}, {}", ra_w("rdx"), sh));
        }
        "rdx"
    };
    if rem {
        if q != "rcx" {
            e.line(&format!("mov {}, {}", ra_w("rcx"), ra_w(q)));
        }
        let imm = if bits == 32 { (ku as u32 as i32) as i64 } else { ku as i64 };
        e.line(&format!("imul {}, {}, {}", ra_w("rcx"), ra_w("rcx"), imm));
        e.line(&format!("sub {}, {}", ra_w("rax"), ra_w("rcx")));
        ra.store_dst(e, d, "rax");
    } else {
        ra.store_dst(e, d, q);
    }
    true
}

/// Is `s` a 64-bit machine register name (and therefore an operand whose
/// content other register moves can destroy)?
fn is_reg64(s: &str) -> bool {
    matches!(
        s,
        "rax" | "rbx" | "rcx" | "rdx" | "rsi" | "rdi" | "rbp" | "rsp"
            | "r8" | "r9" | "r10" | "r11" | "r12" | "r13" | "r14" | "r15"
    )
}

/// Emits a **parallel** register move: all pairs `(target, source)` hold
/// AT THE SAME TIME, so a target may at the same time be the source of
/// another pair.
///
/// Necessary because `r8`/`r9` are argument registers 5 and 6 as well as
/// scratch registers of the allocation (`TEMP_REGS`). Moved naively one
/// after another, the fifth parameter would overwrite an argument that the
/// sixth still needs — exactly that bug made `tests/024_six_args.fi` yield
/// 13 instead of 21 without inlining.
///
/// Method: as long as a target exists that no open pair needs as a source
/// any more, that pair is printed at once. If only cycles are left, one of
/// them is broken open through `rax` — `rax` is never the home of a value
/// (neither in `CALLEE_SAVED` nor in `TEMP_REGS`).
fn parallel_reg_moves(e: &mut Emitter, pairs: &[(String, String)]) {
    let mut open: Vec<(String, String)> =
        pairs.iter().filter(|(z, q)| z != q).cloned().collect();
    while !open.is_empty() {
        if let Some(i) = open
            .iter()
            .position(|(z, _)| !open.iter().any(|(_, q)| q == z))
        {
            let (z, q) = open.remove(i);
            e.line(&format!("mov {}, {}", z, q));
            continue;
        }
        // Only cycles left: rescue the old content of the target into rax, so
        // that the target becomes free; all sources that pointed at it read
        // from rax from now on.
        let (z, q) = open[0].clone();
        e.line(&format!("mov rax, {}", z));
        for (_, source) in open.iter_mut() {
            if *source == z {
                *source = "rax".to_string();
            }
        }
        e.line(&format!("mov {}, {}", z, q));
        open.remove(0);
    }
}

fn epilogue(e: &mut Emitter, a: &Alloc) {
    for (r, off) in &a.saved {
        e.line(&format!("mov {}, qword ptr [rbp-{}]", r, off));
    }
    e.line("mov rsp, rbp");
    e.line("pop rbp");
    e.line("ret");
}

fn emit_block(
    e: &mut Emitter,
    ra: &Ra,
    b: &Block,
    next: Option<BlockId>,
    site: &mut crate::panic_rt::SiteCounter,
) -> Result<(), String> {
    // FUSION of `cmp` + conditional jump.
    //
    // Without it every comparison costs seven instructions: `cmp`, `setcc al`,
    // `movzx eax, al`, a copy into the target register, `test`, `jnz`, `jmp`.
    // So the bool value is produced, stored and checked against zero right
    // away. With the fusion there are three: `cmp`, `jcc`, `jmp`.
    //
    // Measured on `lib/html/mem.fi`: the inlined range check of `buf_at`
    // produces exactly this pattern, and the decoder runs through it up to
    // five times per character.
    //
    // Conditions (all needed):
    //   * the LAST instruction of the block is the comparison — only then can
    //     nothing change the flags between `cmp` and jump,
    //   * its result is the jump condition,
    //   * it is read EXACTLY ONCE (otherwise the bool value is needed),
    //   * no `secret` value (SPEC §9.2).
    let mergeable = match (&b.term, b.insts.last()) {
        (Term::BrCond { cond, .. }, Some(last)) => {
            matches!(last.op, Op::Cmp { .. })
                && last.dst == Some(*cond)
                && ra.read.get(*cond as usize).copied().unwrap_or(2) == 1
                && !ra.f.is_secret(*cond)
        }
        _ => false,
    };
    let n = if mergeable { b.insts.len() - 1 } else { b.insts.len() };
    for i in &b.insts[..n] {
        // ROUND 94 -- the register allocated path carries the line table too.
        // Before this round it emitted only the `fn` line, which is why an
        // optimized build claimed the function's first line for its whole
        // body (measured: `inl.fi:7` for code out of `inl.fi:3`).
        e.loc_at(i.loc);
        emit_inst(e, ra, i, site)?;
    }
    if mergeable {
        if let Some(last) = b.insts.last() {
            e.loc_at(last.loc);
        }
        return emit_cmp_br(e, ra, b, next);
    }
    let f = ra.f;
    match &b.term {
        Term::Br(t) => {
            if next != Some(*t) {
                e.line(&format!("jmp {}", block_label(&f.name, *t)));
            }
        }
        Term::Switch { val, ty, .. } => {
            // Round 51: the value travels DIRECTLY from its place to rax.
            // Formerly this path wrote it into its frame slot first, because
            // `emit_switch` could read it only from there — two memory
            // accesses per state change at the tokenizer (10,2 M Ir on
            // realweb).
            //
            // Guarantee to `ValueSource::Loaded`: `Ra::load_ext` ALWAYS emits
            // a write to `eax`/`rax` here. The only branch that would emit
            // nothing is "the source is the target register already" — and
            // `rax` is never handed out (see CALLEE_SAVED / TEMP_REGS /
            // ARG_SPARE / DIV_SPARE). For safety exactly that is checked
            // here.
            let (v, vty) = (*val, *ty);
            if matches!(ra.a.place(v), Loc::Reg("rax")) {
                return Err("internal error: switch value is in rax".to_string());
            }
            crate::codegen_switch::emit_switch(
                e,
                f,
                crate::codegen_switch::ValueSource::Loaded(&|e2: &mut Emitter, bits: u32| {
                    ra.load_ext(e2, "rax", v, vty, bits);
                }),
                &b.term,
            )?;
        }
        Term::BrCond { cond, then_bb, else_bb } => {
            if f.constant_time && f.is_secret(*cond) {
                return Err(format!(
                    "#[constant_time]: conditional jump in '{}' depends on a secret value (%{})",
                    f.name, cond
                ));
            }
            if f.val_ty(*cond) != FTy::Bool {
                return Err(format!(
                    "internal error: condition %{} in '{}' is {}, expected bool",
                    cond,
                    f.name,
                    f.val_ty(*cond).name()
                ));
            }
            let o = ra.opnd_w(*cond, 8);
            if o.contains('[') {
                e.line(&format!("cmp {}, 0", o));
            } else {
                e.line(&format!("test {}, {}", o, o));
            }
            if next == Some(*else_bb) {
                e.line(&format!("jnz {}", block_label(&f.name, *then_bb)));
            } else if next == Some(*then_bb) {
                e.line(&format!("jz {}", block_label(&f.name, *else_bb)));
            } else {
                e.line(&format!("jnz {}", block_label(&f.name, *then_bb)));
                e.line(&format!("jmp {}", block_label(&f.name, *else_bb)));
            }
        }
        Term::Ret(v) => {
            if let Some(v) = v {
                ra.load_full(e, "rax", *v);
            } else {
                // Round 51: NO `xor eax, eax` any more. A function with
                // return type `void` has no result value; System V
                // leaves `rax` undefined in that case, and in FIR
                // nobody reads the result of a void call (`Op::Call` without
                // `dst`). Measured at the tokenizer: 4.229.623 calls, so
                // just as many instructions for nothing.
            }
            epilogue(e, ra.a);
        }
        Term::Unset => {
            return Err(format!(
                "internal error: block bb{} in '{}' has no terminator",
                b.id, f.name
            ))
        }
    }
    Ok(())
}

/// `cmp` and conditional jump in one: the comparison of the last instruction
/// of the block sets the flags, the terminator reads them immediately.
fn emit_cmp_br(e: &mut Emitter, ra: &Ra, b: &Block, next: Option<BlockId>) -> Result<(), String> {
    let f = ra.f;
    let last = b.insts.last().ok_or("internal error: empty block at cmp+jcc")?;
    let (op, oty, a, bb) = match &last.op {
        Op::Cmp { op, ty, a, b } => (*op, *ty, *a, *b),
        _ => return Err("internal error: cmp+jcc without comparison".to_string()),
    };
    let (then_bb, else_bb) = match &b.term {
        Term::BrCond { then_bb, else_bb, .. } => (*then_bb, *else_bb),
        _ => return Err("internal error: cmp+jcc without brcond".to_string()),
    };
    let bits = oty.bits().max(8);
    let oa = ra.opnd_w(a, bits);
    let ob = ra.opnd_w(bb, bits);
    if ra.a.imm(a).is_some() || (oa.contains('[') && ob.contains('[')) {
        ra.load_full(e, "rax", a);
        e.line(&format!("cmp {}, {}", rn("rax", bits), ob));
    } else {
        e.line(&format!("cmp {}, {}", oa, ob));
    }
    let jcc = match (op, oty.signed()) {
        (CmpOp::Eq, _) => "je",
        (CmpOp::Ne, _) => "jne",
        (CmpOp::Lt, true) => "jl",
        (CmpOp::Lt, false) => "jb",
        (CmpOp::Le, true) => "jle",
        (CmpOp::Le, false) => "jbe",
        (CmpOp::Gt, true) => "jg",
        (CmpOp::Gt, false) => "ja",
        (CmpOp::Ge, true) => "jge",
        (CmpOp::Ge, false) => "jae",
    };
    if next == Some(else_bb) {
        e.line(&format!("{} {}", jcc, block_label(&f.name, then_bb)));
    } else if next == Some(then_bb) {
        e.line(&format!("{} {}", jcc_inverse(jcc), block_label(&f.name, else_bb)));
    } else {
        e.line(&format!("{} {}", jcc, block_label(&f.name, then_bb)));
        e.line(&format!("jmp {}", block_label(&f.name, else_bb)));
    }
    Ok(())
}

/// The counter jump (fallthrough optimization: target and fallthrough swap).
fn jcc_inverse(jcc: &str) -> &'static str {
    match jcc {
        "je" => "jne",
        "jne" => "je",
        "jl" => "jge",
        "jge" => "jl",
        "jb" => "jae",
        "jae" => "jb",
        "jle" => "jg",
        "jg" => "jle",
        "jbe" => "ja",
        "ja" => "jbe",
        _ => unreachable!("unknown jump {}", jcc),
    }
}

fn emit_inst(
    e: &mut Emitter,
    ra: &Ra,
    i: &Inst,
    site: &mut crate::panic_rt::SiteCounter,
) -> Result<(), String> {
    let ty = i.ty;
    match &i.op {
        // ROUND 82: only the three vector instructions with a SCALAR result
        // reach this path (`supported()` keeps the rest away). They compute in
        // rax/rcx, which are never the home of a value here.
        Op::Simd { kind, args, .. } => match kind {
            crate::simd::SimdKind::Crc32U8 => {
                let d = i.dst.ok_or("internal error: crc32 without target")?;
                ra.load_full(e, "rax", args[0]);
                ra.load_full(e, "rcx", args[1]);
                e.line("crc32 eax, cl");
                ra.store_dst(e, d, "rax");
            }
            crate::simd::SimdKind::Crc32U64 => {
                let d = i.dst.ok_or("internal error: crc32 without target")?;
                ra.load_full(e, "rax", args[0]);
                ra.load_full(e, "rcx", args[1]);
                e.line("crc32 rax, rcx");
                ra.store_dst(e, d, "rax");
            }
            crate::simd::SimdKind::CpuFeatures => {
                let d = i.dst.ok_or("internal error: cpu_features without target")?;
                crate::simd::emit_cpuid_pub(e);
                ra.store_dst(e, d, "rax");
            }
            _ => return Err("internal error: v128 reached the register path".to_string()),
        },
        Op::Const(c) => {
            let d = i.dst.ok_or("internal error: const without target")?;
            if ra.a.imm(d).is_some() {
                return Ok(()); // stands as immediate at every use site
            }
            let val = ty.truncate(*c) as i64;
            match ra.a.loc(d) {
                Loc::Reg(r) => {
                    if val == 0 {
                        e.line(&format!("xor {}, {}", rn(r, 32), rn(r, 32)));
                    } else {
                        e.line(&format!("mov {}, {}", r, val));
                    }
                }
                Loc::Slot(_) => {
                    if val == 0 {
                        e.line("xor eax, eax");
                    } else {
                        e.line(&format!("mov rax, {}", val));
                    }
                    ra.store_dst(e, d, "rax");
                }
            }
        }
        Op::Bin(op, x, y) => {
            let d = i.dst.ok_or("internal error: binary operation without target")?;
            // Round 51: address computation that sits in the following memory
            // access (`add` as address forming, `shl`/`mul` as scaling of the index).
            if let Some(src) = ra.preloader.get(&d).copied() {
                // Of the computation only filling the register is left.
                if let Loc::Reg(r) = ra.a.loc(d) {
                    ra.load_full(e, r, src);
                    return Ok(());
                }
            }
            if ra.offset.contains_key(&d) || ra.skipped.contains(&d) {
                return Ok(()); // is read nowhere else
            }
            emit_bin(e, ra, *op, ty, *x, *y, d)?;
        }
        // ROUND 72 -- checked "+ - *" (SPEC section 13, item L9). Loaded
        // fresh into rax/rcx here (unlike the preloader path above, which
        // this instruction never takes -- the caller's overflow test needs
        // BOTH operands sitting in known registers, so nothing about this
        // one may be folded into an address computation).
        Op::CheckedBin { op, a, b, msg } => {
            let d = i.dst.ok_or("internal error: checked binary operation without target")?;
            // ROUND 90, stage 2d -- the DIRECT form. See `checked_direct`.
            if checked_direct(e, ra, *op, ty, *a, *b, d, msg, site) {
                return Ok(());
            }
            ra.load_ext(e, "rax", *a, ty, 64);
            ra.load_ext(e, "rcx", *b, ty, 64);
            crate::panic_rt::emit_checked_bin(e, *op, ty, msg, site, &|e: &mut Emitter| {
                // ROUND 90: the failure arm reloads instead of finding the
                // values on the stack. `rcx` before `rdx`: if `b` lives in
                // rdx the first load rescues it, and `a` can never live in
                // rdx at a checked operation (`op_pins`).
                ra.load_ext(e, "rcx", *b, ty, 64);
                ra.load_ext(e, "rdx", *a, ty, 64);
            });
            ra.store_dst(e, d, "rax");
        }
        Op::CheckedDiv { op, a, b, msg_zero, msg_range } => {
            let d = i.dst.ok_or("internal error: checked division without target")?;
            // ROUND SPEED -- a CONSTANT divisor makes both checks dead.
            // `b == 0` is decided at compile time, and the signed special
            // case `MIN / -1` needs `b == -1`; `emit_div_const` refuses both
            // constants, so reaching it means neither check can ever fire.
            // What is left is the division, and it does not need `div`.
            if let Some(k) = ra.a.imm(*b) {
                if emit_div_const(e, ra, *op, ty, *a, k, d) {
                    return Ok(());
                }
            }
            ra.load_ext(e, "rax", *a, ty, 64);
            ra.load_ext(e, "rcx", *b, ty, 64);
            crate::panic_rt::emit_checked_div(e, *op, ty, msg_zero, msg_range, site, &|e: &mut Emitter| {
                ra.load_ext(e, "rcx", *b, ty, 64);
                ra.load_ext(e, "rdx", *a, ty, 64);
            });
            let res = if *op == BinOp::Div { "rax" } else { "rdx" };
            ra.store_dst(e, d, res);
        }
        Op::CheckedCast { src, from, msg } => {
            let d = i.dst.ok_or("internal error: checked cast without target")?;
            ra.load_ext(e, "rax", *src, *from, 64);
            crate::panic_rt::emit_checked_cast(e, *from, ty, msg, site, &|e: &mut Emitter| {
                ra.load_ext(e, "rdx", *src, *from, 64);
            });
            ra.store_dst(e, d, "rax");
        }
        // ROUND 89 -- the checked ARRAY INDEX (SPEC section 13, item L9).
        // The index is a `usize`, so ONE unsigned comparison against the
        // length decides both ends at once.
        Op::CheckedIdx { idx, len, msg } => {
            let d = i.dst.ok_or("internal error: checked index without target")?;
            ra.load_ext(e, "rax", *idx, ty, 64);
            crate::panic_rt::emit_checked_idx(e, *len, msg, site);
            ra.store_dst(e, d, "rax");
        }
        // ROUND 72 -- explicit "+% -% *%" / "+| -| *|" (SPEC section 13,
        // item L9): never checked, the caller's own well-defined fallback.
        Op::BinWrapSat { kind, op, a, b } => {
            let d = i.dst.ok_or("internal error: wrap/sat binary operation without target")?;
            emit_wrap_sat_ra(e, ra, *kind, *op, ty, *a, *b, d, site)?;
        }
        Op::Cmp { op, ty: oty, a, b } => {
            let d = i.dst.ok_or("internal error: comparison without target")?;
            let bits = oty.bits().max(8);
            let oa = ra.opnd_w(*a, bits);
            let ob = ra.opnd_w(*b, bits);
            // `cmp` tolerates at most one memory operand and no immediate on
            // the left.
            if ra.a.imm(*a).is_some() || (oa.contains('[') && ob.contains('[')) {
                ra.load_full(e, "rax", *a);
                e.line(&format!("cmp {}, {}", rn("rax", bits), ob));
            } else {
                e.line(&format!("cmp {}, {}", oa, ob));
            }
            let signed = oty.signed();
            let cc = match (op, signed) {
                (CmpOp::Eq, _) => "sete",
                (CmpOp::Ne, _) => "setne",
                (CmpOp::Lt, true) => "setl",
                (CmpOp::Lt, false) => "setb",
                (CmpOp::Le, true) => "setle",
                (CmpOp::Le, false) => "setbe",
                (CmpOp::Gt, true) => "setg",
                (CmpOp::Gt, false) => "seta",
                (CmpOp::Ge, true) => "setge",
                (CmpOp::Ge, false) => "setae",
            };
            e.line(&format!("{} al", cc));
            // widen directly into the target register — `rax` is never handed
            // out, which is why `al` is always free here.
            match ra.a.loc(d) {
                Loc::Reg(dr) => e.line(&format!("movzx {}, al", rn(dr, 32))),
                Loc::Slot(_) => {
                    e.line("movzx eax, al");
                    ra.store_dst(e, d, "rax");
                }
            }
        }
        Op::Un(op, x) => {
            let d = i.dst.ok_or("internal error: unary operation without target")?;
            let bits = if ty.bits() > 32 { 64 } else { 32 };
            ra.load_full(e, "rax", *x);
            match op {
                UnOp::Neg => e.line(&format!("neg {}", rn("rax", bits))),
                UnOp::Not => {
                    if ty == FTy::Bool {
                        e.line("xor eax, 1");
                    } else {
                        e.line(&format!("not {}", rn("rax", bits)));
                    }
                }
            }
            ra.store_dst(e, d, "rax");
        }
        Op::Cast { src, from } => {
            let d = i.dst.ok_or("internal error: conversion without target")?;
            if ty == FTy::Bool {
                let bits = from.bits().max(8);
                let o = ra.opnd_w(*src, bits);
                e.line(&format!("cmp {}, 0", o));
                e.line("setne al");
                e.line("movzx eax, al");
            } else if let Loc::Reg(dr) = ra.a.loc(d) {
                // ROUND SPEED -- straight into the target register.
                //
                // Every conversion used to go `mov rax, <src>` and then
                // `mov <dst>, rax`, two instructions for what is one move
                // or one `movzx`. `Op::Load` has taken the direct route
                // since round 51; the conversion right next to it did not.
                // `bench/firn/bytecount.fi`, the fill loop, had
                // `mov r10, rax / mov rax, r10 / mov r11, rax` in it --
                // three moves where one belongs.
                //
                // Reading the source and writing the target in one
                // instruction is safe even when they are the same register:
                // x86 reads the operands before it writes the result, and
                // `load_ext` emits nothing at all when source and target
                // already agree.
                ra.load_ext(e, dr, *src, *from, 64);
                return Ok(());
            } else {
                ra.load_ext(e, "rax", *src, *from, 64);
            }
            ra.store_dst(e, d, "rax");
        }
        Op::GcAddr { regs } => {
            let d = i.dst.ok_or("internal error: gc_state without target")?;
            crate::codegen_x86::emit_gc_addr(e, *regs);
            ra.store_dst(e, d, "rax");
        }
        Op::Alloca { .. } => {
            let d = i.dst.ok_or("internal error: alloca without target")?;
            if ra.a.cell(d).is_some() || ra.a.frame_addr.contains_key(&d) {
                return Ok(()); // promoted or directly addressed cell
            }
            let off = ra
                .a
                .frame
                .alloca_off
                .get(d as usize)
                .copied()
                .flatten()
                .ok_or("internal error: alloca without space")?;
            e.line(&format!("lea rax, [rbp-{}]", off));
            ra.store_dst(e, d, "rax");
        }
        Op::Load { addr } => {
            let d = i.dst.ok_or("internal error: load without target")?;
            if ra.a.alias.contains_key(&d) {
                // Cell alias: the value is already in the cell register,
                // the only use reads it directly through loc().
                return Ok(());
            }
            let bits = ty.bits().max(8);
            if let Some((r, _)) = ra.a.cell(*addr) {
                // Cell in the register: pull out only the relevant width,
                // directly into the target register when possible.
                let t = match ra.a.loc(d) {
                    Loc::Reg(dr) => dr,
                    Loc::Slot(_) => "rax",
                };
                match bits {
                    8 => e.line(&format!("movzx {}, {}", rn(t, 32), rn(r, 8))),
                    16 => e.line(&format!("movzx {}, {}", rn(t, 32), rn(r, 16))),
                    32 => e.line(&format!("mov {}, {}", rn(t, 32), rn(r, 32))),
                    _ => {
                        if t != r {
                            e.line(&format!("mov {}, {}", t, r));
                        }
                    }
                }
                if t != "rax" {
                    return Ok(());
                }
            } else {
                let mem = match (ra.offset.get(addr), ra.a.frame_addr.get(addr), ra.a.place(*addr)) {
                    (Some(addr), _, _) => addr.text(),
                    (None, Some(off), _) => format!("[rbp-{}]", off),
                    (None, None, Loc::Reg(r)) => format!("[{}]", r),
                    (None, None, Loc::Slot(_)) => {
                        ra.load_full(e, "rcx", *addr);
                        "[rcx]".to_string()
                    }
                };
                // Load DIRECTLY into the target register instead of going
                // through rax and copying afterwards. `mov r9, qword ptr [r9]`
                // is correct: the instruction reads the address before it
                // writes the target. That saves one instruction per memory
                // access — in the loop body of matmul those were two of 24.
                let zr = match ra.a.loc(d) {
                    Loc::Reg(r) => r,
                    Loc::Slot(_) => "rax",
                };
                match bits {
                    8 => e.line(&format!("movzx {}, byte ptr {}", rn(zr, 32), mem)),
                    16 => e.line(&format!("movzx {}, word ptr {}", rn(zr, 32), mem)),
                    32 => e.line(&format!("mov {}, dword ptr {}", rn(zr, 32), mem)),
                    _ => e.line(&format!("mov {}, qword ptr {}", zr, mem)),
                }
                if zr != "rax" {
                    return Ok(());
                }
            }
            ra.store_dst(e, d, "rax");
        }
        Op::Store { addr, val } => {
            let bits = ty.bits().max(8);
            if let Some((r, _)) = ra.a.cell(*addr) {
                // The full width is always copied; only the lower `bits`
                // bits are read (uniform access width).
                let o = ra.opnd(*val);
                if o != r {
                    e.line(&format!("mov {}, {}", r, o));
                }
            } else {
                let mem = match (ra.offset.get(addr), ra.a.frame_addr.get(addr), ra.a.place(*addr)) {
                    (Some(addr), _, _) => addr.text(),
                    (None, Some(off), _) => format!("[rbp-{}]", off),
                    (None, None, Loc::Reg(r)) => format!("[{}]", r),
                    (None, None, Loc::Slot(_)) => {
                        ra.load_full(e, "rcx", *addr);
                        "[rcx]".to_string()
                    }
                };
                let o = ra.opnd_w(*val, bits);
                if o.contains('[') {
                    e.line(&format!("mov {}, {}", rn("rax", bits), o));
                    e.line(&format!("mov {} {}, {}", size_word(bits), mem, rn("rax", bits)));
                } else {
                    e.line(&format!("mov {} {}, {}", size_word(bits), mem, o));
                }
            }
        }
        Op::PtrAdd { base, off } => {
            let d = i.dst.ok_or("internal error: ptradd without target")?;
            if let Some(src) = ra.preloader.get(&d).copied() {
                if let Loc::Reg(r) = ra.a.loc(d) {
                    ra.load_full(e, r, src);
                    return Ok(());
                }
            }
            if ra.offset.contains_key(&d) {
                // The offset sits in the following memory access; the address
                // itself is read nowhere else and needs no `lea`.
                return Ok(());
            }
            // `lea` reads BOTH operands before it writes the target — so a
            // collision between target and offset register is harmless there.
            // Only the `mov`+`add` way needs the detour through rax; that is
            // why the target is chosen optimistically here and taken back in
            // the two `add` branches alone.
            let dreg = match ra.a.loc(d) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => "rax",
            };
            let reg_of = |v: Val| match (ra.a.imm(v), ra.a.place(v)) {
                (None, Loc::Reg(r)) => Some(r),
                _ => None,
            };
            let off_reg = reg_of(*off);
            let base_reg = reg_of(*base);
            let mut target = dreg;
            if let Some(boff) = ra.a.frame_addr.get(base).copied() {
                // address = rbp - boff + off  -> a single `lea`
                match (ra.a.imm(*off), off_reg) {
                    (Some(k), _) => {
                        let delta = k - boff as i64;
                        if delta >= 0 {
                            e.line(&format!("lea {}, [rbp+{}]", target, delta));
                        } else {
                            e.line(&format!("lea {}, [rbp-{}]", target, -delta));
                        }
                    }
                    (None, Some(r)) => e.line(&format!("lea {}, [rbp+{}-{}]", target, r, boff)),
                    (None, None) => {
                        e.line(&format!("mov rcx, {}", ra.opnd(*off)));
                        e.line(&format!("lea {}, [rbp+rcx-{}]", target, boff));
                    }
                }
            } else if let (Some(x), Some(k)) = (base_reg, ra.a.imm(*off)) {
                lea_sum(e, target, x, k);
            } else if let (Some(x), Some(y)) = (base_reg, off_reg) {
                e.line(&format!("lea {}, [{}+{}]", target, x, y));
            } else if target != "rax" {
                // The base lies in the frame or is a constant: fetch it to
                // rax once, then ONE `lea` into the target.
                ra.load_full(e, "rax", *base);
                match (ra.a.imm(*off), off_reg) {
                    (Some(k), _) => lea_sum(e, target, "rax", k),
                    (None, Some(y)) => e.line(&format!("lea {}, [rax+{}]", target, y)),
                    (None, None) => {
                        e.line(&format!("add rax, {}", ra.opnd(*off)));
                        target = "rax";
                    }
                }
            } else {
                ra.load_full(e, "rax", *base);
                e.line(&format!("add rax, {}", ra.opnd(*off)));
                target = "rax";
            }
            if target == "rax" {
                ra.store_dst(e, d, "rax");
            }
        }
        Op::Call { name, args } => {
            // Arguments into the argument registers. The allocation does hand
            // `r8` and `r9` out as a home (`TEMP_REGS`), which is why the
            // register to register moves have to happen IN PARALLEL; operands
            // from memory or immediate constants read no register and come
            // afterwards.
            // Arguments from the seventh on lie on the stack at `call`
            // ([rsp], [rsp+8], …). They are put down FIRST: after that the
            // argument registers are free and are not touched any more. As
            // intermediate storage serves `rax` (never the home of a value);
            // the sources are rbp relative or registers and stay untouched by
            // `sub rsp`.
            //
            // ALIGNMENT: at the `call` boundary `rsp` must be 16-fold aligned.
            // After `push rbp` + `sub rsp, <multiple of 16>` it is; the argument
            // area is therefore rounded up to 16 as well — word for word like
            // the base path in codegen_x86.rs.
            let stack = args.len().saturating_sub(ARG_REGS.len());
            let space = align_up(stack as u64 * 8, 16);
            if space > 0 {
                e.line(&format!("sub rsp, {}", space));
                for (k, arg) in args.iter().skip(ARG_REGS.len()).enumerate() {
                    ra.load_full(e, "rax", *arg);
                    e.line(&format!("mov qword ptr [rsp+{}], rax", k * 8));
                }
            }
            let mut reg_moves: Vec<(String, String)> = Vec::new();
            let mut later: Vec<(usize, Val)> = Vec::new();
            for (k, arg) in args.iter().enumerate().take(ARG_REGS.len()) {
                let o = ra.opnd(*arg);
                if is_reg64(&o) {
                    reg_moves.push((ARG_REGS[k].to_string(), o));
                } else {
                    later.push((k, *arg));
                }
            }
            parallel_reg_moves(e, &reg_moves);
            for (k, arg) in later {
                ra.load_full(e, ARG_REGS[k], arg);
            }
            e.line(&format!("call {}", label(name)));
            if space > 0 {
                e.line(&format!("add rsp, {}", space));
            }
            if let Some(d) = i.dst {
                ra.store_dst(e, d, "rax");
            }
        }
        // Dynamic dispatch (iface.rs, round 46). Word for word like the `call`
        // above, only the target sits in a register instead of in a
        // symbol. The target is loaded LAST, namely into `rax`: `rax` is never
        // the home of a value (see the head of this file) and no argument
        // register — so the load can destroy neither an argument already set
        // nor the target itself.
        Op::CallIndirect { target, args } => {
            let stack = args.len().saturating_sub(ARG_REGS.len());
            let space = align_up(stack as u64 * 8, 16);
            if space > 0 {
                e.line(&format!("sub rsp, {}", space));
                for (k, arg) in args.iter().skip(ARG_REGS.len()).enumerate() {
                    ra.load_full(e, "rax", *arg);
                    e.line(&format!("mov qword ptr [rsp+{}], rax", k * 8));
                }
            }
            let mut reg_moves: Vec<(String, String)> = Vec::new();
            let mut later: Vec<(usize, Val)> = Vec::new();
            for (k, arg) in args.iter().enumerate().take(ARG_REGS.len()) {
                let o = ra.opnd(*arg);
                if is_reg64(&o) {
                    reg_moves.push((ARG_REGS[k].to_string(), o));
                } else {
                    later.push((k, *arg));
                }
            }
            parallel_reg_moves(e, &reg_moves);
            for (k, arg) in later {
                ra.load_full(e, ARG_REGS[k], arg);
            }
            ra.load_full(e, "rax", *target);
            e.line("call rax");
            if space > 0 {
                e.line(&format!("add rsp, {}", space));
            }
            if let Some(d) = i.dst {
                ra.store_dst(e, d, "rax");
            }
        }
        Op::VtabAddr { table } => {
            let d = i.dst.ok_or("internal error: vtab without target")?;
            e.line(&format!(
                "lea rax, [rip + {}]",
                crate::iface::table_label(table)
            ));
            ra.store_dst(e, d, "rax");
        }
        // Round 58 (fnval.rs), like `VtabAddr`: `rax` is never the home of a
        // value, so the address may be built there.
        Op::FnRef { name } => {
            let d = i.dst.ok_or("internal error: fnref without target")?;
            e.line(&format!(
                "lea rax, [rip + {}]",
                crate::fnval::record_label(name)
            ));
            ra.store_dst(e, d, "rax");
        }
        // Round 89 (statics.rs), like `FnRef`: a link time constant address.
        Op::GlobalAddr { name } => {
            let d = i.dst.ok_or("internal error: globaladdr without target")?;
            e.line(&format!(
                "lea rax, [rip + {}]",
                crate::statics::label_of(name)
            ));
            ra.store_dst(e, d, "rax");
        }
        Op::Syscall { args } => {
            const SYS_REGS: [&str; 6] = ["rdi", "rsi", "rdx", "r10", "r8", "r9"];
            if args.is_empty() {
                return Err("internal error: syscall without number".to_string());
            }
            // The same class of bug as with the call: `r10`, `r8` and `r9`
            // are at the same time scratch registers of the allocation.
            let mut sys_moves: Vec<(String, String)> = Vec::new();
            let mut sys_later: Vec<(usize, Val)> = Vec::new();
            for (k, arg) in args.iter().skip(1).enumerate() {
                let o = ra.opnd(*arg);
                if is_reg64(&o) {
                    sys_moves.push((SYS_REGS[k].to_string(), o));
                } else {
                    sys_later.push((k, *arg));
                }
            }
            parallel_reg_moves(e, &sys_moves);
            for (k, arg) in sys_later {
                ra.load_full(e, SYS_REGS[k], arg);
            }
            ra.load_full(e, "rax", args[0]);
            e.line("syscall");
            if let Some(d) = i.dst {
                ra.store_dst(e, d, "rax");
            }
        }
        Op::Select { cond, a, b } => {
            // SPEC §9.2: always `cmov`, never a jump.
            let d = i.dst.ok_or("internal error: select without target")?;
            ra.load_full(e, "rdx", *cond);
            ra.load_full(e, "rax", *b);
            ra.load_full(e, "rcx", *a);
            e.line("test dl, dl");
            e.line("cmovnz rax, rcx");
            ra.store_dst(e, d, "rax");
        }
        // ROUND 92 -- THE INSTRUCTION THE WHOLE PHI ELIMINATION COMES DOWN TO.
        //
        // `phi.rs` puts one of these at the end of every predecessor of a
        // block that had a phi. How much it costs is decided HERE: when the
        // allocator gave both ends the same register the copy is free and
        // disappears completely, when it gave them two registers it is one
        // `mov`, and only when one end sits in the frame does it touch
        // memory. That is why the loop counter of round 92 costs nothing
        // even though FIR now writes a copy per back edge.
        Op::Copy { src } => {
            let d = i.dst.ok_or("internal error: copy without target")?;
            match ra.a.loc(d) {
                // Straight into its home. `load_full` writes nothing at all
                // when the value already stands there, so a copy the
                // allocator has coalesced by accident costs no instruction.
                Loc::Reg(r) => ra.load_full(e, r, *src),
                Loc::Slot(_) => {
                    ra.load_full(e, "rax", *src);
                    ra.store_dst(e, d, "rax");
                }
            }
        }
        Op::Phi { .. } => {
            return Err("internal error: phi in the code generator (phi.rs did not run)".into())
        }
        Op::Barrier { val } => {
            let d = i.dst.ok_or("internal error: barrier without target")?;
            ra.load_full(e, "rax", *val);
            e.raw("    # barrier: opaque to every optimization pass");
            ra.store_dst(e, d, "rax");
        }
        Op::SecureZero { addr, size } => {
            ra.load_full(e, "rdi", *addr);
            ra.load_full(e, "rcx", *size);
            e.line("xor eax, eax");
            e.line("cld");
            e.line("rep stosb");
        }
        // Round 49 (thread.rs). As with the atomic addition, rax/rcx/rdx are
        // never the home of a value; on top of that `spawn` uses the system
        // call registers and is entered above as call alike, so that no
        // interval in a caller-saved register lives across it.
        Op::AtomicCas { addr, erw, new } => {
            let d = i.dst.ok_or("internal error: atomcas without target")?;
            ra.load_full(e, "rcx", *addr);
            ra.load_full(e, "rdx", *new);
            ra.load_full(e, "rax", *erw);
            crate::thread::cas_sequence(e);
            ra.store_dst(e, d, "rax");
        }
        Op::ThreadSpawn { arg, stack, ctid } => {
            let d = i.dst.ok_or("internal error: spawn without target")?;
            ra.load_full(e, "rdi", *arg);
            ra.load_full(e, "rsi", *stack);
            ra.load_full(e, "rdx", *ctid);
            crate::thread::spawn_sequence(e);
            ra.store_dst(e, d, "rax");
        }
        Op::ThreadSelf => {
            let d = i.dst.ok_or("internal error: threadself without target")?;
            crate::thread::self_sequence(e);
            ra.store_dst(e, d, "rax");
        }
        Op::AtomicAdd { addr, val } => {
            // Round 47: ONE instruction, with a `lock` prefix. rax and rcx are
            // never the home of a value (neither CALLEE_SAVED nor TEMP_REGS nor
            // ARG_SPARE/DIV_SPARE), which is why this instruction needs no
            // entry in memop_pos/divsel_pos.
            let d = i.dst.ok_or("internal error: atomadd without target")?;
            ra.load_full(e, "rcx", *addr);
            ra.load_full(e, "rax", *val);
            e.line("lock xadd qword ptr [rcx], rax");
            ra.store_dst(e, d, "rax");
        }
        Op::CopyMem { dst, src, size } => {
            ra.load_full(e, "rdi", *dst);
            ra.load_full(e, "rsi", *src);
            e.line(&format!("mov rcx, {}", size));
            e.line("cld");
            e.line("rep movsb");
        }
        // ROUND 52: unreachable — `unsupported_basic` sends every function
        // containing inline assembler or MMIO to the base path. As an error
        // rather than a silent branch, so that a later loosening flies up.
        Op::Asm { .. } | Op::MmioLoad { .. } | Op::MmioStore { .. } => {
            return Err(
                "internal error: inline assembler/MMIO in the register-allocating path".to_string(),
            )
        }
    }
    Ok(())
}

/// `lea target, [base + offset]` — the address computation of the processor
/// as an arithmetic unit. The gain is no cosmetics: `mov d, a` + `add d, b`
/// are two instructions and destroy `d`, `lea d, [a+b]` is one and only reads.
/// That way half the register shuffling in address computations
/// (`base + i*width`) disappears — in `bench/firn/matmul.fi` 14 of the 27
/// instructions of the inner loop were pure register copies.
///
/// **64 bits only.** For 32-bit targets `add eax, ecx` zeroes the upper 32
/// bits, `lea rax, [rcx+rdx]` does not — the difference becomes visible as
/// soon as the value is passed on as a 64-bit value. That is why the narrow
/// case stays with the old way.
///
/// **Flags.** `lea` sets none, `add` does. That is harmless here: in FIR
/// every comparison is an `Op::Cmp` of its own that produces its
/// `cmp`/`setcc` immediately one after the other. No `setcc`, `jcc` or
/// `cmov` ever reads the flags of a FIR arithmetic operation.
/// Does exactly one operand lie in a register and the other in the frame
/// (no immediate)? Then the way through rax with a closing `lea` pays off.
fn add_over_rax(ra: &Ra, a: Val, b: Val) -> bool {
    let is_reg = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.place(v), Loc::Reg(_));
    let is_frame = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.place(v), Loc::Slot(_));
    (is_reg(a) && is_frame(b)) || (is_frame(a) && is_reg(b))
}

/// Can `d = a op b` be written as a single `lea`?
///
/// 64 bits only (see `lea_sum`), only with a target register, and only when
/// the operands really do qualify as address parts: register + register,
/// register + immediate, immediate + register. For `sub` additionally
/// `k != i64::MIN`, because `-k` would overflow otherwise.
fn lea_possible(ra: &Ra, op: BinOp, ty: FTy, a: Val, b: Val, d: Val) -> bool {
    if ty.bits() <= 32 || !matches!(ra.a.loc(d), Loc::Reg(_)) {
        return false;
    }
    let is_reg = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.place(v), Loc::Reg(_));
    match op {
        BinOp::Add => {
            (is_reg(a) && is_reg(b))
                || (is_reg(a) && ra.a.imm(b).is_some())
                || (ra.a.imm(a).is_some() && is_reg(b))
        }
        BinOp::Sub => is_reg(a) && matches!(ra.a.imm(b), Some(k) if k != i64::MIN),
        _ => false,
    }
}

fn lea_sum(e: &mut Emitter, target: &str, base: &str, offset: i64) {
    if offset >= 0 {
        e.line(&format!("lea {}, [{}+{}]", target, base, offset));
    } else {
        e.line(&format!("lea {}, [{}-{}]", target, base, -(offset as i128) as i64));
    }
}

fn emit_bin(
    e: &mut Emitter,
    ra: &Ra,
    op: BinOp,
    ty: FTy,
    a: Val,
    b: Val,
    d: Val,
) -> Result<(), String> {
    let wide = ty.bits() > 32;
    let bits = if wide { 64 } else { 32 };
    match op {
        BinOp::Mul if ra.a.imm(b).is_some() => {
            // `imul` has no two operand form with an immediate; powers of two
            // turn into a shift.
            let k = ra.a.imm(b).unwrap_or(1);
            let dst_reg = match (ra.a.loc(d), ra.a.loc(a)) {
                (Loc::Reg(r), _) => r,
                (Loc::Slot(_), _) => "rax",
            };
            // ROUND SPEED -- `lea` instead of `mov` + `shl`.
            //
            // Scaling an index is the most frequent multiplication there
            // is, and `lea` does it in ONE instruction with a free target:
            // `[x*4]` for four, `[x+x*2]` for three, `[x+x*4]` for five,
            // `[x+x*8]` for nine. It needs the source in a register (an
            // immediate has been folded long before) and 64 bit operands --
            // a 32 bit `lea` would have to be written `[eax*4]`, which
            // assembles but computes with a 32 bit address size, and that
            // is a different question from a 32 bit result.
            //
            // Where it bites: `bench/firn/matmul.fi` at `release-safe`. The
            // checked pointer addition blocks the address folding, so the
            // `* 4` of every index really is emitted, twice per iteration
            // of the innermost loop, as `mov rdx, rsi` + `shl rdx, 2`.
            let src_reg = match (ra.a.imm(a), ra.a.place(a)) {
                (None, Loc::Reg(r)) => Some(r),
                _ => None,
            };
            if wide {
                if let Some(x) = src_reg {
                    let form = match k {
                        2 => Some(format!("[{}+{}]", x, x)),
                        3 => Some(format!("[{}+{}*2]", x, x)),
                        4 => Some(format!("[{}*4]", x)),
                        5 => Some(format!("[{}+{}*4]", x, x)),
                        8 => Some(format!("[{}*8]", x)),
                        9 => Some(format!("[{}+{}*8]", x, x)),
                        _ => None,
                    };
                    if let Some(f) = form {
                        e.line(&format!("lea {}, {}", dst_reg, f));
                        if dst_reg == "rax" {
                            ra.store_dst(e, d, "rax");
                        }
                        return Ok(());
                    }
                }
            }
            let shift = if k > 1 && (k & (k - 1)) == 0 { Some(k.trailing_zeros()) } else { None };
            match shift {
                Some(sh) => {
                    ra.load_full(e, dst_reg, a);
                    e.line(&format!("shl {}, {}", rn(dst_reg, bits), sh));
                }
                None => {
                    e.line(&format!("imul {}, {}, {}", rn(dst_reg, bits), ra.opnd_w(a, bits), k))
                }
            }
            if dst_reg == "rax" {
                ra.store_dst(e, d, "rax");
            }
        }
        // `lea` instead of `mov`+`add`: one instruction, no destroyed target,
        // and it works even when the second operand already lies in the target
        // register — there the old way fell back to the rax detour with THREE
        // instructions.
        //
        // The condition checks the lea case COMPLETELY. There is deliberately no
        // fallback path here: everything else falls into the branch below,
        // which brings its own protection along (if the second operand sits in
        // the target register, the computation has to go through rax). A
        // fallback path without that protection produced
        // `mov r9, [rbp-8]` + `add r9, r9` on the first attempt — matmul ran
        // into a memory access fault. That bug is the reason for this form.
        BinOp::Add | BinOp::Sub if lea_possible(ra, op, ty, a, b, d) => {
            let dr = match ra.a.loc(d) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => unreachable!("lea_possible requires a target register"),
            };
            let reg_of = |v: Val| match (ra.a.imm(v), ra.a.place(v)) {
                (None, Loc::Reg(r)) => Some(r),
                _ => None,
            };
            match op {
                BinOp::Add => match (reg_of(a), reg_of(b), ra.a.imm(a), ra.a.imm(b)) {
                    (Some(x), Some(y), _, _) => e.line(&format!("lea {}, [{}+{}]", dr, x, y)),
                    (Some(x), None, _, Some(k)) => lea_sum(e, dr, x, k),
                    (None, Some(y), Some(k), _) => lea_sum(e, dr, y, k),
                    _ => unreachable!("lea_possible has guaranteed the case"),
                },
                _ => match (reg_of(a), ra.a.imm(b)) {
                    (Some(x), Some(k)) => lea_sum(e, dr, x, -k),
                    _ => unreachable!("lea_possible has guaranteed the case"),
                },
            }
        }
        // One operand lies in the frame, the other in a register — by far the
        // most frequent case in address computations (`base + offset`, where
        // the base is a parameter in the frame). Fetch it to rax once, then
        // ONE `lea` into the target. The general branch below needs three
        // instructions here, because the target coincides with the register
        // operand and it therefore has to compute through rax and copy back.
        BinOp::Add if wide && matches!(ra.a.loc(d), Loc::Reg(_)) && add_over_rax(ra, a, b) => {
            let dr = match ra.a.loc(d) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => unreachable!("excluded by the condition"),
            };
            let is_reg = |v: Val| ra.a.imm(v).is_none() && matches!(ra.a.place(v), Loc::Reg(_));
            // `+` is commutative: the register operand becomes the index part.
            let (out_frame, in_reg) = if is_reg(b) { (a, b) } else { (b, a) };
            let y = match ra.a.place(in_reg) {
                Loc::Reg(r) => r,
                Loc::Slot(_) => unreachable!("add_via_rax has reserved a register"),
            };
            ra.load_full(e, "rax", out_frame);
            e.line(&format!("lea {}, [rax+{}]", dr, y));
        }
        BinOp::Add | BinOp::Sub | BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Mul => {
            let m = match op {
                BinOp::Add => "add",
                BinOp::Sub => "sub",
                BinOp::And => "and",
                BinOp::Or => "or",
                BinOp::Xor => "xor",
                _ => "imul",
            };
            // compute directly in the target register when possible
            if let Loc::Reg(dr) = ra.a.loc(d) {
                let ob = ra.opnd_w(b, bits);
                if ob != rn(dr, bits) {
                    ra.load_full(e, dr, a);
                    e.line(&format!("{} {}, {}", m, rn(dr, bits), ob));
                    return Ok(());
                }
            }
            ra.load_full(e, "rax", a);
            e.line(&format!("{} {}, {}", m, rn("rax", bits), ra.opnd_w(b, bits)));
            ra.store_dst(e, d, "rax");
        }
        BinOp::Div | BinOp::Rem => {
            // ROUND SPEED -- dividing by a CONSTANT without `div`.
            if let Some(k) = ra.a.imm(b) {
                if emit_div_const(e, ra, op, ty, a, k, d) {
                    return Ok(());
                }
            }
            ra.load_ext(e, "rax", a, ty, bits);
            ra.load_ext(e, "rcx", b, ty, bits);
            if ty.signed() {
                if wide {
                    e.line("cqo");
                    e.line("idiv rcx");
                } else {
                    e.line("cdq");
                    e.line("idiv ecx");
                }
            } else {
                e.line("xor edx, edx");
                if wide {
                    e.line("div rcx");
                } else {
                    e.line("div ecx");
                }
            }
            let res = if op == BinOp::Div { "rax" } else { "rdx" };
            ra.store_dst(e, d, res);
        }
        BinOp::Shl | BinOp::Shr => {
            let m = match (op, ty.signed()) {
                (BinOp::Shl, _) => "shl",
                (_, true) => "sar",
                (_, false) => "shr",
            };
            if let Some(k) = ra.a.imm(b) {
                // constant distance: immediate form, no rcx build-up.
                // Mask like the CPU (32 bits: 5 bits, 64 bits: 6 bits); FIR does
                // not let widths >= the bit width through the optimizer in the
                // first place, but the mask keeps the assembler text within the
                // imm8 frame.
                let k = k & if bits == 64 { 63 } else { 31 };
                // shift directly in the target register when it has one
                match ra.a.loc(d) {
                    Loc::Reg(dr) => {
                        ra.load_ext(e, dr, a, ty, bits);
                        e.line(&format!("{} {}, {}", m, rn(dr, bits), k));
                    }
                    Loc::Slot(_) => {
                        ra.load_ext(e, "rax", a, ty, bits);
                        e.line(&format!("{} {}, {}", m, rn("rax", bits), k));
                        ra.store_dst(e, d, "rax");
                    }
                }
            } else {
                ra.load_ext(e, "rax", a, ty, bits);
                ra.load_full(e, "rcx", b);
                e.line(&format!("{} {}, cl", m, rn("rax", bits)));
                ra.store_dst(e, d, "rax");
            }
        }
    }
    Ok(())
}

/// **ROUND 90, STAGE 2d** — a checked `+` or `-` computed IN THE TARGET
/// REGISTER, with the second operand as an immediate or a memory operand.
///
/// The unchecked path has done this since round 51 (`emit_bin` below): if
/// the result has a register, `add r13, 1` is the whole instruction. The
/// checked path did not — it loaded BOTH operands into rax/rcx first,
/// because the failure arm has to print them and `panic_rt` was written
/// around that pair of registers. So `k = k + 1` inside a loop cost
///
///     mov rax, r12 / mov rcx, 1 / add rax, rcx / jc site / mov r12, rax
///
/// where `release-fast` needs `lea r12, [r12+1]`. Four instructions of
/// difference on the counter of every loop in the program, and nothing in
/// them was the check.
///
/// This is the same shape as the unchecked one plus the branch:
///
///     add r12, 1 / jc site
///
/// The failure arm cannot reload `a` any more when `a` lived in the target
/// register — it has just been overwritten. It does not have to: for `+`
/// the original is `d - b`, for `-` it is `d + b`, both exact in two's
/// complement at the width the operation was carried out in. The arm
/// recomputes it, out of line, on the path that never returns.
///
/// Returns `false` when the shape does not fit (no target register, the
/// second operand living in the target register, a multiplication); the
/// caller then emits the rax/rcx form exactly as before.
#[allow(clippy::too_many_arguments)]
fn checked_direct(
    e: &mut Emitter,
    ra: &Ra,
    op: BinOp,
    ty: FTy,
    a: Val,
    b: Val,
    d: Val,
    msg: &str,
    site: &mut crate::panic_rt::SiteCounter,
) -> bool {
    if !matches!(op, BinOp::Add | BinOp::Sub) {
        return false;
    }
    if std::env::var_os("FIRN_NO_CHECKED_DIRECT").is_some() {
        return false;
    }
    let dr = match ra.a.loc(d) {
        Loc::Reg(r) => r,
        Loc::Slot(_) => return false,
    };
    // The second operand may not live where the result is about to be
    // written: `add r12, r12` would read the half finished value, and the
    // failure arm could not reload it either.
    if matches!(ra.a.place(b), Loc::Reg(r) if r == dr) {
        return false;
    }
    let bits = ty.bits().max(8);
    let ob = ra.opnd_w(b, bits);
    if ob == rn(dr, bits) {
        return false;
    }
    // An immediate wider than 32 bits has no `add r64, imm` form.
    if let Some(k) = ra.a.imm(b) {
        if i32::try_from(k).is_err() {
            return false;
        }
    }
    ra.load_ext(e, dr, a, ty, bits);
    let m = if op == BinOp::Add { "add" } else { "sub" };
    e.line(&format!("{} {}, {}", m, rn(dr, bits), ob));
    crate::panic_rt::emit_checked_tail(e, op, ty, msg, site, &|e: &mut Emitter| {
        // rcx first: `b` still lives where it did, the operation touched
        // only `dr`.
        ra.load_ext(e, "rcx", b, ty, 64);
        // and `a` back out of the result -- the one thing the stack rescue
        // of round 72 was really for.
        e.line(&format!("mov rdx, {}", dr));
        let inv = if op == BinOp::Add { "sub" } else { "add" };
        e.line(&format!("{} {}, {}", inv, rn("rdx", bits), rn("rcx", bits)));
        if bits < 64 {
            if bits == 32 {
                if ty.signed() {
                    e.line("movsxd rdx, edx");
                } else {
                    e.line("mov edx, edx");
                }
            } else {
                let ext = if ty.signed() { "movsx" } else { "movzx" };
                e.line(&format!("{} rdx, {}", ext, rn("rdx", bits)));
            }
        }
    });
    true
}

/// **ROUND 72** -- `+% -% *%` (wrapping) and `+| -| *|` (saturating), SPEC
/// section 13 item L9, register allocator aware path. Mirrors
/// `codegen_x86.rs::emit_wrap_sat` instruction for instruction; the two
/// exist separately because the two backends load operands through
/// different APIs (`ra.load_ext` here, the free function `load_ext` there)
/// and neither can call into the other's private frame representation.
#[allow(clippy::too_many_arguments)]
fn emit_wrap_sat_ra(
    e: &mut Emitter,
    ra: &Ra,
    kind: crate::fir::WrapSatKind,
    op: BinOp,
    ty: FTy,
    a: Val,
    b: Val,
    d: Val,
    site: &mut crate::panic_rt::SiteCounter,
) -> Result<(), String> {
    if kind == crate::fir::WrapSatKind::Wrap {
        // Bit for bit the unchecked path: wrapping on overflow is exactly
        // what plain two's complement arithmetic already means.
        return emit_bin(e, ra, op, ty, a, b, d);
    }
    let bits = ty.bits();
    ra.load_ext(e, "rax", a, ty, 64);
    ra.load_ext(e, "rcx", b, ty, 64);
    e.line("push rax");
    e.line("push rcx");
    match op {
        BinOp::Add => e.line(&format!("add {}, {}", rn("rax", bits), rn("rcx", bits))),
        BinOp::Sub => e.line(&format!("sub {}, {}", rn("rax", bits), rn("rcx", bits))),
        // 8 bit has no two operand `imul` (see panic_rt.rs::emit_checked_bin).
        BinOp::Mul if ty.signed() && bits == 8 => e.line("imul cl"),
        BinOp::Mul if ty.signed() => {
            e.line(&format!("imul {}, {}", rn("rax", bits), rn("rcx", bits)))
        }
        BinOp::Mul => e.line(&format!("mul {}", rn("rcx", bits))),
        _ => return Err("internal error: wrap/sat only defined for + - *".to_string()),
    }
    // Same reasoning as panic_rt.rs::emit_checked_bin: unsigned overflow
    // is a CF condition for every one of + - *, not only Mul -- `200u8 +
    // 100u8` sets CF but leaves OF clear (as i8 the sum still fits), so an
    // `Add`/`Sub` clamp gated on OF alone silently returned the wrapped
    // value instead of saturating (round 72's own bug, found testing
    // `+|` on `u8`).
    let jcc = if ty.signed() { "jo" } else { "jc" };
    // ROUND 72, second pass: unique per FUNCTION, not per value number --
    // see codegen_x86.rs::emit_wrap_sat for the collision this replaces.
    let uid = site.next();
    let clamp = format!(".Lsatclamp{}", uid);
    let done = format!(".Lsatdone{}", uid);
    e.line(&format!("{} {}", jcc, clamp));
    // Success: the two rescued words are not needed again, drop them.
    e.line("add rsp, 16");
    e.line(&format!("jmp {}", done));
    e.raw(&format!("{}:", clamp));
    // Recover the two ORIGINAL operands for the sign tests below.
    e.line("pop rcx");
    e.line("pop rax");
    let (min_lit, max_lit): (i128, i128) = match ty {
        FTy::I8 => (i8::MIN as i128, i8::MAX as i128),
        FTy::I16 => (i16::MIN as i128, i16::MAX as i128),
        FTy::I32 => (i32::MIN as i128, i32::MAX as i128),
        FTy::I64 => (i64::MIN as i128, i64::MAX as i128),
        FTy::U8 => (0, u8::MAX as i128),
        FTy::U16 => (0, u16::MAX as i128),
        FTy::U32 => (0, u32::MAX as i128),
        _ => (0, u64::MAX as i128),
    };
    if !ty.signed() {
        if op == BinOp::Sub {
            e.line(&format!("mov rax, {}", min_lit as u64));
        } else {
            e.line(&format!("mov rax, {}", max_lit as u64));
        }
    } else {
        match op {
            BinOp::Add => {
                e.line("cmp rax, 0");
                e.line(&format!("mov rax, {}", max_lit));
                e.line(&format!("mov rdx, {}", min_lit));
                e.line("cmovl rax, rdx");
            }
            BinOp::Sub => {
                e.line("cmp rcx, 0");
                e.line(&format!("mov rax, {}", min_lit));
                e.line(&format!("mov rdx, {}", max_lit));
                e.line("cmovl rax, rdx");
            }
            BinOp::Mul => {
                e.line("xor rax, rcx");
                e.line("cmp rax, 0");
                e.line(&format!("mov rax, {}", max_lit));
                e.line(&format!("mov rdx, {}", min_lit));
                e.line("cmovl rax, rdx");
            }
            _ => unreachable!("guarded above"),
        }
    }
    e.raw(&format!("{}:", done));
    ra.store_dst(e, d, "rax");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_x86::emit;
    use crate::fir::{Module, Term};

    /// Loop with a counter in an `alloca`: the counter has to land in a
    /// register (cell promotion), not on the stack.
    fn loop_func() -> Func {
        let mut f = Func::new("main", vec![], FTy::I32);
        let head = f.add_block();
        let body = f.add_block();
        let exit = f.add_block();
        let slot = f.alloca(4, 4);
        let zero = f.push(0, FTy::I32, Op::Const(0));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: zero });
        f.set_term(0, Term::Br(head));
        let i = f.push(head, FTy::I32, Op::Load { addr: slot });
        let ten = f.push(head, FTy::I32, Op::Const(10));
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a: i, b: ten });
        f.set_term(head, Term::BrCond { cond: c, then_bb: body, else_bb: exit });
        let i2 = f.push(body, FTy::I32, Op::Load { addr: slot });
        let one = f.push(body, FTy::I32, Op::Const(1));
        let s = f.push(body, FTy::I32, Op::Bin(BinOp::Add, i2, one));
        f.push_void(body, FTy::I32, Op::Store { addr: slot, val: s });
        f.set_term(body, Term::Br(head));
        let r = f.push(exit, FTy::I32, Op::Load { addr: slot });
        f.set_term(exit, Term::Ret(Some(r)));
        f
    }

    #[test]
    fn loop_counter_lands_in_a_register() {
        let f = loop_func();
        let a = allocate(&f);
        assert!(!a.cells.is_empty(), "the alloca cell must be promoted");
        let regs = a.locs.iter().filter(|l| matches!(l, Loc::Reg(_))).count() + a.cells.len();
        assert!(regs >= 3, "too few registers assigned: {}", regs);
    }

    #[test]
    fn loop_body_without_mem_access() {
        let asm = emit(&Module { funcs: vec![loop_func()] }).expect("codegen");
        // in the body (bb2) no [rbp- may appear any more
        let body = asm.split(".Lmain__bb2:").nth(1).unwrap_or("");
        let body = body.split(".Lmain__bb3:").next().unwrap_or("");
        assert!(!body.contains("[rbp-"), "loop body still accesses the stack:\n{}", body);
    }

    #[test]
    fn callee_saved_become_saved_and_retrieved() {
        let f = loop_func();
        let a = allocate(&f);
        if a.saved.is_empty() {
            return; // nothing to save -> nothing to check
        }
        let asm = emit(&Module { funcs: vec![loop_func()] }).expect("codegen");
        for (r, off) in &a.saved {
            assert!(asm.contains(&format!("mov qword ptr [rbp-{}], {}", off, r)), "{}", asm);
            assert!(asm.contains(&format!("mov {}, qword ptr [rbp-{}]", r, off)), "{}", asm);
        }
    }

    #[test]
    fn cells_with_escaping_address_become_not_promoted() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let slot = f.alloca(8, 8);
        let off = f.push(0, FTy::I64, Op::Const(0));
        let p = f.push(0, FTy::Ptr, Op::PtrAdd { base: slot, off });
        let v = f.push(0, FTy::I32, Op::Const(7));
        f.push_void(0, FTy::I32, Op::Store { addr: p, val: v });
        let l = f.push(0, FTy::I32, Op::Load { addr: slot });
        f.set_term(0, Term::Ret(Some(l)));
        let a = allocate(&f);
        assert!(a.cells.is_empty(), "address escapes via ptradd");
    }

    #[test]
    fn secret_values_get_no_register() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let c = f.push(0, FTy::I32, Op::Const(5));
        f.secret.insert(c);
        let d = f.push(0, FTy::I32, Op::Bin(BinOp::Add, c, c));
        f.set_term(0, Term::Ret(Some(d)));
        let a = allocate(&f);
        assert!(matches!(a.loc(c), Loc::Slot(_)));
    }

    #[test]
    fn select_stays_cmov_also_with_registers() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let c = f.push(0, FTy::Bool, Op::Call { name: "g".into(), args: vec![] });
        let x = f.push(0, FTy::I32, Op::Const(1));
        let y = f.push(0, FTy::I32, Op::Const(2));
        let s = f.push(0, FTy::I32, Op::Select { cond: c, a: x, b: y });
        f.set_term(0, Term::Ret(Some(s)));
        let mut g = Func::new("g", vec![], FTy::Bool);
        let t = g.push(0, FTy::Bool, Op::Const(1));
        g.set_term(0, Term::Ret(Some(t)));
        let asm = emit(&Module { funcs: vec![f, g] }).expect("codegen");
        assert!(asm.contains("cmovnz"), "{}", asm);
    }

    /// Round 43: more than six parameters are NO reason for the base path any
    /// more — the seventh comes from [rbp+16].
    #[test]
    fn many_parameter_stay_in_register_path() {
        let mut f = Func::new("f", vec![FTy::I64; 7], FTy::I64);
        f.set_term(0, Term::Ret(Some(6)));
        assert!(supported(&f));
        let mut e = Emitter::default();
        emit_func_ra(&mut e, &f).expect("register path responsible").expect("codegen");
        assert!(e.out.contains("qword ptr [rbp+16]"), "{}", e.out);
    }

    /// … and a call with eight arguments puts the last two on the stack
    /// without violating the 16-byte alignment.
    #[test]
    fn call_with_eight_args_puts_two_on_the_stack() {
        let mut g = Func::new("main", vec![], FTy::I32);
        let mut args = Vec::new();
        for k in 0..8 {
            args.push(g.push(0, FTy::I64, Op::Const(k as i128 + 1)));
        }
        let r = g.push(0, FTy::I64, Op::Call { name: "f".to_string(), args });
        let rc = g.push(0, FTy::I32, Op::Cast { src: r, from: FTy::I64 });
        g.set_term(0, Term::Ret(Some(rc)));
        assert!(supported(&g));
        let mut e = Emitter::default();
        emit_func_ra(&mut e, &g).expect("register path responsible").expect("codegen");
        assert!(e.out.contains("sub rsp, 16"), "{}", e.out);
        assert!(e.out.contains("mov qword ptr [rsp+0], rax"), "{}", e.out);
        assert!(e.out.contains("mov qword ptr [rsp+8], rax"), "{}", e.out);
        assert!(e.out.contains("add rsp, 16"), "{}", e.out);
    }
    // ---------------------------------------------------------- Round 51 ---

    /// `[base + index*4]` instead of `shl` + `lea` + access.
    #[test]
    fn addressing_moves_in_the_mem_operands() {
        let mut f = Func::new("main", vec![FTy::Ptr, FTy::U64], FTy::U64);
        let four = f.push(0, FTy::U64, Op::Const(4));
        let sk = f.push(0, FTy::U64, Op::Bin(BinOp::Mul, 1, four));
        let ad = f.push(0, FTy::U64, Op::Bin(BinOp::Add, 0, sk));
        let w = f.push(0, FTy::U32, Op::Load { addr: ad });
        let c = f.push(0, FTy::U64, Op::Cast { src: w, from: FTy::U32 });
        f.set_term(0, Term::Ret(Some(c)));
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        let body = asm.split("main:").nth(1).unwrap();
        assert!(
            body.lines().any(|l| l.contains("dword ptr [") && l.contains("*4]")),
            "no scaled memory operand:\n{}",
            asm
        );
        assert!(!body.contains("shl "), "scaling remained:\n{}", asm);
        assert!(!body.contains("lea "), "address computation remained:\n{}", asm);
    }

    /// If the same address is read TWICE, it must not travel into the
    /// memory operand — otherwise the base lives longer than the allocator
    /// knows about (the class of bug from round 40/41).
    #[test]
    fn twice_read_address_becomes_not_folded() {
        let mut f = Func::new("main", vec![FTy::Ptr, FTy::U64], FTy::U64);
        let ad = f.push(0, FTy::U64, Op::Bin(BinOp::Add, 0, 1));
        let a = f.push(0, FTy::U64, Op::Load { addr: ad });
        let b = f.push(0, FTy::U64, Op::Load { addr: ad });
        let sum = f.push(0, FTy::U64, Op::Bin(BinOp::Add, a, b));
        f.set_term(0, Term::Ret(Some(sum)));
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        let body = asm.split("main:").nth(1).unwrap();
        assert!(
            body.contains("lea ") || body.lines().filter(|l| l.contains("add ")).count() > 0,
            "address should be computed once:\n{}",
            asm
        );
    }

    /// A 32-bit `add` must NOT become addressing: there FIR cuts the result
    /// off, the addressing would not.
    #[test]
    fn narrow_add_becomes_not_to_address() {
        let mut f = Func::new("main", vec![FTy::Ptr, FTy::U32], FTy::U32);
        let ad = f.push(0, FTy::U32, Op::Bin(BinOp::Add, 0, 1));
        let w = f.push(0, FTy::U32, Op::Load { addr: ad });
        f.set_term(0, Term::Ret(Some(w)));
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        let body = asm.split("main:").nth(1).unwrap();
        // The 32-bit addition has to stand there as an instruction OF ITS OWN.
        // Which register the allocator picks for it is its own business: the
        // narrow view is called `eax`..`edi`, but `r8d`..`r15d` for the
        // extended ones (the merge of round 49 shifted the choice to `r10d` —
        // the old test looked for "add e" only and therefore struck although
        // the code was right).
        let narrow_addition = body.lines().any(|l| {
            let l = l.trim();
            l.starts_with("add e")
                || (l.starts_with("add r") && l.split(',').next().is_some_and(|r| r.ends_with('d')))
        });
        assert!(
            narrow_addition || body.contains("lea "),
            "32-bit addition must remain its own instruction:\n{}",
            asm
        );
    }

    /// The value of a `switch` comes from its register, not through the
    /// frame — and the index needs no `mov eax, eax`.
    #[test]
    fn switch_reads_the_value_without_detour_over_the_frame() {
        let mut f = Func::new("main", vec![FTy::U32], FTy::I32);
        let mut cases = Vec::new();
        for i in 0..12i128 {
            let b = f.add_block();
            let c = f.push(b, FTy::I32, Op::Const(i));
            f.set_term(b, Term::Ret(Some(c)));
            cases.push((i, b));
        }
        let bd = f.add_block();
        let cd = f.push(bd, FTy::I32, Op::Const(99));
        f.set_term(bd, Term::Ret(Some(cd)));
        f.set_term(0, Term::Switch { val: 0, ty: FTy::U32, cases, default: bd });
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        assert!(asm.contains("jmp qword ptr [rdx + rax*8]"), "{}", asm);
        assert!(!asm.contains("mov eax, eax"), "superfluous zero extension:\n{}", asm);
        let body = asm.split("main:").nth(1).unwrap();
        // The value is not written into its frame slot first.
        assert!(
            !body.lines().any(|l| l.trim().starts_with("mov qword ptr [rbp-") && l.contains(", rax")),
            "switch value went out of range:\n{}",
            asm
        );
    }

    /// Block layout: behind a conditional jump no unconditional one may stand
    /// any more when one of the two edges can be a fallthrough.
    #[test]
    fn blocklayout_makes_out_the_second_jump_a_fallthrough() {
        let f = loop_func();
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        let lines: Vec<&str> = asm.lines().map(|l| l.trim()).collect();
        for (i, z) in lines.iter().enumerate() {
            let conditional = z.starts_with('j') && !z.starts_with("jmp");
            if conditional {
                if let Some(n) = lines.get(i + 1) {
                    assert!(
                        !n.starts_with("jmp "),
                        "unconditional jump after conditional:\n{}",
                        asm
                    );
                }
            }
        }
    }

    /// A `void` function no longer sets `rax` to zero.
    #[test]
    fn void_ret_without_xor() {
        let mut empty = Func::new("empty", vec![], FTy::Void);
        empty.set_term(0, Term::Ret(None));
        let mut m = Func::new("main", vec![], FTy::I32);
        let n = m.push(0, FTy::I32, Op::Const(7));
        m.set_term(0, Term::Ret(Some(n)));
        let asm = emit(&Module { funcs: vec![empty, m] }).expect("codegen");
        let body = asm.split("_F0.empty:").nth(1).unwrap();
        let body = body.split("main:").next().unwrap();
        assert!(!body.contains("xor eax, eax"), "{}", asm);
        assert!(body.contains("ret"), "{}", asm);
    }

    // ---------------------------------------------- ROUND 90: the clobbers ---

    fn inst(ty: FTy, op: Op) -> Inst {
        Inst::new(Some(0), ty, op)
    }

    /// THE TABLE OF ROUND 90. Every entry is an instruction whose emitted
    /// code writes a register its operand list does not name, or one that
    /// deliberately does NOT. Getting a single line of this wrong is a
    /// wrong-code bug on `release-safe` and `dev-fast`, which is what
    /// happened: see the module note at `inst_clobbers`.
    #[test]
    fn implicit_clobbers_are_modelled() {
        let msg = || "m".to_string();
        // The instruction of the bug: one-operand `mul rcx` -> rdx:rax.
        for ty in [FTy::U16, FTy::U32, FTy::U64] {
            let i = inst(ty, Op::CheckedBin { op: BinOp::Mul, a: 1, b: 2, msg: msg() });
            assert!(inst_clobbers(&i) & M_RDX != 0, "checked u{} `*` must claim rdx", ty.bits());
        }
        // `mul cl` computes in ax alone, `imul rax, rcx` writes its target.
        assert_eq!(
            inst_clobbers(&inst(FTy::U8, Op::CheckedBin { op: BinOp::Mul, a: 1, b: 2, msg: msg() })),
            0
        );
        assert_eq!(
            inst_clobbers(&inst(FTy::I64, Op::CheckedBin { op: BinOp::Mul, a: 1, b: 2, msg: msg() })),
            0
        );
        // Checked `+`/`-` write nothing but rax. Before round 90 they banned
        // rdx wholesale through `crosses_divsel`.
        for op in [BinOp::Add, BinOp::Sub] {
            assert_eq!(inst_clobbers(&inst(FTy::U64, Op::CheckedBin { op, a: 1, b: 2, msg: msg() })), 0);
        }
        // A checked `as` keeps the original in rdx to compare the round trip
        // against (round 90; round 72 pushed it on the stack instead).
        assert_eq!(
            inst_clobbers(&inst(FTy::U8, Op::CheckedCast { src: 1, from: FTy::U64, msg: msg() })),
            M_RDX
        );
        // Division, checked and unchecked: the remainder register.
        for op in [BinOp::Div, BinOp::Rem] {
            assert_eq!(inst_clobbers(&inst(FTy::U64, Op::Bin(op, 1, 2))), M_RDX);
            let d = Op::CheckedDiv { op, a: 1, b: 2, msg_zero: msg(), msg_range: msg() };
            assert_eq!(inst_clobbers(&inst(FTy::I64, d)), M_RDX);
        }
        // `cmov` fetches the condition into rdx first, `cmpxchg` uses it as
        // its third operand.
        assert_eq!(inst_clobbers(&inst(FTy::U64, Op::Select { cond: 1, a: 2, b: 3 })), M_RDX);
        assert_eq!(inst_clobbers(&inst(FTy::U64, Op::AtomicCas { addr: 1, erw: 2, new: 3 })), M_RDX);
        // `rep movsb`/`rep stosb`: rdi and rsi -- and NOT rdx.
        assert_eq!(
            inst_clobbers(&inst(FTy::U64, Op::CopyMem { dst: 1, src: 2, size: 8 })),
            M_RDI | M_RSI
        );
        assert_eq!(inst_clobbers(&inst(FTy::U64, Op::CopyMem { dst: 1, src: 2, size: 8 })) & M_RDX, 0);
        // Wrapping is the unchecked path bit for bit; saturating clamps
        // through rdx (signed) or multiplies through `mul` (unsigned).
        use crate::fir::WrapSatKind;
        assert_eq!(
            inst_clobbers(&inst(
                FTy::U64,
                Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Mul, a: 1, b: 2 }
            )),
            0
        );
        assert_eq!(
            inst_clobbers(&inst(
                FTy::I64,
                Op::BinWrapSat { kind: WrapSatKind::Sat, op: BinOp::Add, a: 1, b: 2 }
            )),
            M_RDX
        );
        assert_eq!(
            inst_clobbers(&inst(
                FTy::U64,
                Op::BinWrapSat { kind: WrapSatKind::Sat, op: BinOp::Mul, a: 1, b: 2 }
            )),
            M_RDX
        );
        // A call destroys every caller-saved register and NO callee-saved
        // one -- that is what makes an interval across a call allocatable at
        // all.
        let c = inst(FTy::U64, Op::Call { name: "f".into(), args: vec![] });
        assert_eq!(inst_clobbers(&c), M_CALL);
        for r in ["rbx", "r12", "r13", "r14", "r15"] {
            assert_eq!(inst_clobbers(&c) & reg_bit(r), 0, "a call must not claim {}", r);
        }
        for r in ["rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"] {
            assert!(inst_clobbers(&c) & reg_bit(r) != 0, "a call must claim {}", r);
        }
    }

    /// `POOL` and `reg_bit` have to describe the same twelve registers as
    /// the four hand-out pools -- and rax/rcx must be in none of them.
    #[test]
    fn the_pool_is_the_four_pools() {
        let mut seen: RegMask = 0;
        for r in CALLEE_SAVED.iter().chain(TEMP_REGS.iter()).chain(ARG_SPARE.iter()).chain(DIV_SPARE.iter()) {
            let b = reg_bit(r);
            assert!(b != 0, "{} is handed out but has no bit", r);
            assert_eq!(seen & b, 0, "{} lies in two pools", r);
            seen |= b;
        }
        assert_eq!(seen, (1 << POOL.len()) - 1, "pool and hand-out pools disagree");
        assert_eq!(reg_bit("rax"), 0);
        assert_eq!(reg_bit("rcx"), 0);
    }

    /// The end of the bug, at the level of the allocation: a value that
    /// lives ACROSS a checked unsigned multiplication may not be in rdx,
    /// and one that only lives across a checked ADDITION may.
    #[test]
    fn a_value_across_a_checked_mul_loses_rdx() {
        // Enough long-lived values that the four temp registers are gone
        // and rdx is really the next one in line.
        let build = |op: BinOp| -> Vec<Loc> {
            let mut f = Func::new("m", vec![FTy::U64; 6], FTy::U64);
            let x = f.push(0, FTy::U64, Op::CheckedBin { op, a: 0, b: 1, msg: "m".into() });
            // every parameter is read again AFTER the checked operation
            let mut acc = x;
            for p in 0..6u32 {
                acc = f.push(0, FTy::U64, Op::Bin(BinOp::Add, acc, p));
            }
            f.set_term(0, Term::Ret(Some(acc)));
            let a = allocate(&f);
            (0..6).map(|v| a.loc(v as Val)).collect()
        };
        let mul = build(BinOp::Mul);
        assert!(
            !mul.iter().any(|l| *l == Loc::Reg("rdx")),
            "a value survives `mul` in rdx: {:?}",
            mul
        );
        let add = build(BinOp::Add);
        assert!(
            add.iter().any(|l| *l == Loc::Reg("rdx")),
            "a checked `+` writes no rdx, so rdx must still be handed out: {:?}",
            add
        );
    }
}
