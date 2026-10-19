// SPDX-License-Identifier: GPL-2.0-only
//! **Round BILLIG — register allocation for aarch64.**
//!
//! Up to this round `codegen_a64.rs` gave every FIR value its own eight-byte
//! slot in the frame and computed in `x9`–`x15`. Its own header says what
//! that is worth: *"That is not fast, and it is not meant to be — it is
//! checkable."* On x86 the linear scan of `regalloc.rs` (round 43) bought
//! **−26 % executed instructions and about +80 % throughput**. On ARM64 —
//! Justin's phone — that gain has been lying on the floor since round 80,
//! and it applies to the WHOLE browser, not only to JavaScript.
//!
//! ## Why this is a new file and not a second target inside `regalloc.rs`
//!
//! `regalloc.rs` is 5005 lines. Counted by function boundaries, about 1580
//! of them are the analysis (liveness, intervals, loop depth, promotable
//! cells, linear scan) and about 3000 are an **x86 assembler emitter** —
//! `rn()`, `Address::text()`, `emit_inst`, `emit_bin`, `emit_div_const`,
//! `descriptor_peephole`. The analysis is machine independent; the emitter
//! is not, and A64 already has one. So the useful half gets reused
//! (`regalloc::compute_live`, `loop_depth`, `promotable_cells`) and only
//! the register description and the scan are written here.
//!
//! ## The register set, and why it is exactly six
//!
//! AAPCS64 gives ten callee-saved general purpose registers, `x19`–`x28`.
//! Six of them are handed out here, `x19`–`x24`, and the reason is the
//! collector and not the ABI:
//!
//! > `codegen_a64::emit_gc_addr` writes **x19–x24** into the state block of
//! > the collector at every `Op::GcAddr { regs: true }` site, because
//! > SPEC §3.5.3 promises a conservative scan over stack **and registers**.
//! > The block has room for six words (`gc::REG_SAVE_OFF`, the same six the
//! > x86 side fills with `rbx`, `rbp`, `r12`–`r15`).
//!
//! A pointer in `x25`–`x28` would be invisible to that scan, the collector
//! would free an object that is still live, and the mistake would show up
//! as a corrupted heap somewhere else entirely. Six registers that are
//! already scanned are worth more than ten that are not. Widening the block
//! is a separate, deliberate change to three files (`gc.rs`, the runtime
//! text and `codegen_x86.rs`) and is not smuggled in here.
//!
//! Everything the scan hands out is callee-saved, so **no interval needs a
//! crossing analysis**: `bl`, `svc` and every helper sequence of this
//! backend leave `x19`–`x24` alone. That is the one place where ARM64 is
//! genuinely simpler than x86, where `r11`/`r10`/`rsi`/`rdi`/`rdx` may only
//! be handed to intervals that cross nothing (`regalloc.rs::exact_crossings`,
//! 90 lines of round 87).
//!
//! ## What deliberately does NOT get a register
//!
//! * `f32`/`f64`/`v128` values — they live in `d`/`q` registers and this
//!   round does not touch the floating point path.
//! * `secret` values (SPEC §9.2) — they stay in memory so that
//!   `SecureZero` can reach them.
//! * every value of a function with `#[interrupt]` — `INT_SAVE_A64` saves
//!   `x0`–`x18` and `x30` and relies on the statement *"x19–x28 are
//!   callee-saved and this backend never hands them out"*. This round makes
//!   that statement false, so interrupt handlers keep the old path.

use crate::fir::{FTy, Func, Inst, Op, Term, Val};
use std::collections::HashMap;

/// The registers handed out. Callee-saved (AAPCS64) AND covered by the
/// register save of `emit_gc_addr` — see the header.
pub(crate) const A64_POOL: [&str; 6] = ["x19", "x20", "x21", "x22", "x23", "x24"];

/// r841: the callee-saved registers that are NOT part of the collector's state block. They are handed out ONLY in a
/// function that is a LEAF in the sense of `is_leaf` (no call, no system call, no `GcAddr`, no atomics, no inline
/// assembler) and that has a loop. Why that is safe:
///
/// * the collector reads x19-x24 at a save point (`emit_gc_addr`), and the world stops only at a safepoint (futex word,
///   `__thread_safepoint`, cooperative). A leaf has neither, so nothing alive in x25-x28 can ever be missed;
/// * nothing called by a leaf can clobber them (it calls nothing; the check blocks end in the panic runtime, which
///   never returns);
/// * the prologue saves and the epilogue restores every one that is used, so the caller sees them unchanged -- and no
///   stale pointer is left behind in them for a later collection to find (the "permanent root" finding of `safepoints`).
///
/// `FIRN_A64_NO_XPOOL=1` switches it off for bisecting.
pub(crate) const A64_XPOOL: [&str; 4] = ["x25", "x26", "x27", "x28"];

/// r841: caller-saved registers that a LEAF with a loop may hand out as well (see `A64_XPOOL`). The emitter touches
/// x0-x8 only in the sequences of calls, system calls, thread creation, `Ret` (x0) and the prologue's move of the incoming
/// arguments; x17 it never touches (x15 is `T` of the checked-arithmetic sequences in panic_rt_a64.rs, so it is NOT handed out; x16 is `U`, the scratch of the checked multiply, and the frame size for
/// big frames). The panic blocks read `x9`/`x10` (and write x0-x3 for the runtime call, which never returns).
/// Consequences: no value of a PARAMETER may live in one of them (the prologue moves the arguments one by one), and
/// x0/x1 stay out (return value / possible second result). Nothing here needs a save: the caller treats them as clobbered.
pub(crate) const A64_XCALLER: [&str; 8] = ["x2", "x3", "x4", "x5", "x6", "x7", "x8", "x17"];

/// ROUND FREG -- the floating point registers handed out: `d8`-`d15`. AAPCS64
/// keeps the LOW 64 bits of `v8`-`v15` across a call, which is exactly what an
/// `f64` (or an `f32` in the low half) needs. Like the integer pool there is
/// no crossing analysis: every call of this backend leaves them alone, and
/// nothing in the runtime text or in `simd_a64` touches `v8`-`v15` (the vector
/// scratch is `v16`-`v21`). Floats are never pointers, so the collector does
/// not care where they sit.
pub(crate) const A64_FPOOL: [&str; 8] = ["d8", "d9", "d10", "d11", "d12", "d13", "d14", "d15"];

/// The result of the allocation. Empty (`!on`) means: everything as before.
#[derive(Default)]
pub(crate) struct RaA64 {
    /// ordinary value -> register
    pub regs: HashMap<Val, &'static str>,
    /// promoted `alloca` -> (register, access width)
    pub cells: HashMap<Val, (&'static str, FTy)>,
    /// which registers the prologue has to save (sorted, as in `A64_POOL`)
    pub saved: Vec<&'static str>,
    /// ROUND FREG -- `f32`/`f64` value -> (its `d` register, is it an `f32`).
    /// An `f32` uses the low half of the same register (`s8` for `d8`).
    pub fregs: HashMap<Val, (&'static str, bool)>,
    /// the `d` registers the prologue has to save (sorted, as in `A64_FPOOL`)
    pub fsaved: Vec<&'static str>,
    /// ROUND BILLIG, THE FINDING WITH THE COLLECTOR -- per `Op::GcAddr
    /// { regs: true }` (value of the target as the key) the registers of
    /// `saved` that hold NOTHING ALIVE there any more. `emit_gc_addr` writes
    /// `xzr` into the state block for them instead of the register.
    ///
    /// WHY IT IS NEEDED, and it is the finding of this round:
    /// `tests/901_dom_tree_gc.fi` went from 0 to 3 with `--no-opt` -- "after
    /// the collection too many objects are still alive". Not wrong code but a
    /// PERMANENT ROOT: the collector's scan is conservative, and a dead
    /// pointer that used to lie in a frame slot of a long returned function
    /// (that is BELOW `sp` and so outside the scan) now lies in a
    /// callee-saved register -- which is written down at every save point.
    /// 4680 nodes stayed alive that way.
    ///
    /// The direction is safe: only a register of `saved` is cleared (the
    /// caller's value of it lies in its own frame and is found there) and only
    /// if no interval covers the point. Intervals are the convex hull of
    /// liveness, i.e. a SUPERSET -- whoever falls through here is really dead.
    /// If the data flow does not reach its fixed point, nothing is cleared.
    pub safepoints: HashMap<Val, Vec<&'static str>>,
    /// ROUND BILLIG, THE OTHER HALF OF THE SAME FINDING -- per position
    /// (block, instruction number) the registers that are overwritten with
    /// `xzr` AFTER it, because a POINTER died there.
    ///
    /// Clearing the state block at the save point is NOT enough: the prologue
    /// of EVERY called function writes the callee-saved registers into its own
    /// frame, and that frame lies above `sp` and is read conservatively. A
    /// dead pointer in x19 would thus propagate down the whole call chain. It
    /// has to vanish where it dies.
    ///
    /// Only for `FTy::Ptr`. A dead number in a register costs the collector
    /// nothing (`__gc_block_of` finds no block for it); a dead pointer costs
    /// the whole tree hanging on it. And only if the register is not needed
    /// again at once -- then the next writer does it for free.
    pub clears: HashMap<(usize, usize), Vec<&'static str>>,
    /// ROUND BILLIG -- values that are REBUILT instead of stored. See
    /// `cheap_const`: a constant that `imm_into` writes in ONE instruction
    /// costs nothing to make again, so it deserves neither a slot nor a
    /// register. Before this the constants of a function ate the register
    /// file: in the smoke test `summe` the four constants 0, 1, 2 and 3 held
    /// x20-x23 while the loop counter went to the stack.
    pub remat: HashMap<Val, i64>,
    /// is the register path active at all?
    pub on: bool,
}

impl RaA64 {
    /// Register of an ordinary value, if it has one.
    pub fn reg(&self, v: Val) -> Option<&'static str> {
        if !self.on {
            return None;
        }
        self.regs.get(&v).copied()
    }
    /// The registers to overwrite with `xzr` AFTER this instruction.
    pub fn clear_after(&self, bi: usize, ii: usize) -> &[&'static str] {
        if !self.on {
            return &[];
        }
        match self.clears.get(&(bi, ii)) {
            Some(v) => v.as_slice(),
            None => &[],
        }
    }
    /// The registers that may be cleared at this save point.
    pub fn dead_at(&self, gcaddr: Val) -> &[&'static str] {
        if !self.on {
            return &[];
        }
        match self.safepoints.get(&gcaddr) {
            Some(v) => v.as_slice(),
            None => &[],
        }
    }
    /// ROUND FREG -- the `d` register of a float value and whether it is an `f32`.
    pub fn freg(&self, v: Val) -> Option<(&'static str, bool)> {
        if !self.on {
            return None;
        }
        self.fregs.get(&v).copied()
    }
    /// The constant a value is, if it is one that gets rebuilt.
    pub fn imm(&self, v: Val) -> Option<i64> {
        if !self.on {
            return None;
        }
        self.remat.get(&v).copied()
    }
    /// Register and width of a promoted cell, if `addr` is one.
    pub fn cell(&self, addr: Val) -> Option<(&'static str, FTy)> {
        if !self.on {
            return None;
        }
        self.cells.get(&addr).copied()
    }
}

/// One live interval.
struct Iv {
    val: Val,
    start: usize,
    end: usize,
    weight: u64,
    is_cell: bool,
    /// r845: the interval spans a barrier (call, system call, ...) -- no caller-saved register
    cross: bool,
    /// r845: the interval spans a point where the world may stop -- no register the collector does not read (x25-x28)
    gc: bool,
}

/// Does `codegen_a64::imm_into` write this value in exactly ONE
/// instruction? Then rebuilding it is never worse than reading it out of a
/// slot, and it is strictly better than holding a register for it.
///
/// The three one instruction forms, in the order `imm_into` tries them:
/// `mov r, xzr` (zero), `movz` (exactly one non-zero 16-bit chunk) and
/// `movn` (at most one chunk that is not 0xffff).
pub(crate) fn cheap_const(v: i64) -> bool {
    let u = v as u64;
    if u == 0 {
        return true;
    }
    let c = [u & 0xffff, (u >> 16) & 0xffff, (u >> 32) & 0xffff, (u >> 48) & 0xffff];
    let nz = c.iter().filter(|x| **x != 0).count();
    let nf = c.iter().filter(|x| **x != 0xffff).count();
    nz == 1 || nf <= 1
}

/// The immediate operand text of `fmov d, #imm8` (`None` if the bit pattern is not one of the 256 values of
/// the form), `"0"` for +0.0 (built from the zero register instead).
pub(crate) fn fp_imm_text(bits: u64, single: bool) -> Option<String> {
    if bits == 0 {
        return Some("0".to_string());
    }
    if single {
        let b = bits as u32;
        let e = (b >> 25) & 0x1f;
        if b & 0x7ffff != 0 || !(e == 0 || e == 0x1f) || ((b >> 30) & 1) == (e & 1) {
            return None;
        }
        Some(format!("{:?}", f32::from_bits(b)))
    } else {
        let e = (bits >> 54) & 0xff;
        if bits & 0xffff_ffff_ffff != 0 || !(e == 0 || e == 0xff) || ((bits >> 62) & 1) == (e & 1) {
            return None;
        }
        Some(format!("{:?}", f64::from_bits(bits)))
    }
}

/// May a value of this type live in a general purpose register?
fn integral(t: FTy) -> bool {
    if t == FTy::Ptr && std::env::var_os("FIRN_A64_NO_PTR").is_some() {
        return false;
    }
    !matches!(t, FTy::F32 | FTy::F64 | FTy::V128 | FTy::Void)
}

/// Values whose address the emitter needs as a FRAME address rather than as
/// a value: none on A64 — `Op::Alloca` produces the address as an ordinary
/// value here. Kept as a function so the reason is written down once.
fn needs_frame_slot(f: &Func, v: Val) -> bool {
    f.is_secret(v)
}

/// Operations the emitter writes as straight code with the scratch registers x9-x16 only (and never as a call or a
/// safepoint). Whitelist: anything else (calls, system calls, `GcAddr`, atomics, threads, `CopyMem`, SIMD, MMIO, inline
/// assembler) is a BARRIER for the allocation, see `allocate`.
fn plain_op(op: &Op) -> bool {
    matches!(
        op,
        Op::Const(_)
            | Op::Bin(..)
            | Op::BinWrapSat { .. }
            | Op::Cmp { .. }
            | Op::Un(..)
            | Op::Cast { .. }
            | Op::CheckedBin { .. }
            | Op::CheckedDiv { .. }
            | Op::CheckedCast { .. }
            | Op::CheckedIdx { .. }
            | Op::CheckedIdxV { .. }
            | Op::Alloca { .. }
            | Op::Load { .. }
            | Op::Store { .. }
            | Op::PtrAdd { .. }
            | Op::Select { .. }
            | Op::GlobalAddr { .. }
            | Op::Copy { .. }
    )
}

/// A function that never calls anything and never reaches a safepoint -- see `A64_XPOOL`.
fn is_leaf(f: &Func) -> bool {
    f.blocks.iter().all(|b| b.insts.iter().all(|i| plain_op(&i.op)))
}

thread_local! {
    /// r845: the functions of the module that can NEVER reach a safepoint (no `GcAddr`, no system call, no atomic, no
    /// thread operation, no inline assembler, no indirect call, no call of anything outside the module -- transitively).
    /// Filled by `compute_silent` at the start of the code generation of a module.
    static SILENT: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// r845: greatest fixed point over the call graph. A function is silent if it consists of plain operations and calls of
/// silent functions only. A silent function leaves x19-x28 alone (it saves what it uses), and nothing in it can stop the
/// world -- so a caller may keep a value (a pointer too) in x25-x28 ACROSS a call of it, although the collector reads
/// only x19-x24 at a save point.
pub(crate) fn compute_silent(m: &crate::fir::Module) {
    use std::collections::{HashMap, HashSet};
    let mut cand: HashSet<&str> = HashSet::new();
    let mut calls: HashMap<&str, Vec<&str>> = HashMap::new();
    for f in &m.funcs {
        if f.interrupt {
            continue;
        }
        let mut ok = true;
        let mut cs: Vec<&str> = Vec::new();
        'scan: for b in &f.blocks {
            for i in &b.insts {
                match &i.op {
                    Op::Call { name, .. } => cs.push(name.as_str()),
                    op if plain_op(op) => {}
                    _ => {
                        ok = false;
                        break 'scan;
                    }
                }
            }
        }
        if ok {
            cand.insert(f.name.as_str());
            calls.insert(f.name.as_str(), cs);
        }
    }
    loop {
        let out: Vec<&str> = cand
            .iter()
            .copied()
            .filter(|n| calls.get(n).map(|cs| cs.iter().any(|c| !cand.contains(c))).unwrap_or(true))
            .collect();
        if out.is_empty() {
            break;
        }
        for n in out {
            cand.remove(n);
        }
    }
    SILENT.with(|s| *s.borrow_mut() = cand.into_iter().map(|x| x.to_string()).collect());
}

/// Does the interval [s, e] contain one of the (sorted) barrier positions? The one exception: the value `val` that IS
/// the result of the call at position `s` (`calls`: position -> result value) -- written from x0 after the call
/// returned. NOTE: `block_start` is the position of the FIRST instruction of the block, so "starts at a call" alone
/// would also match a value that is live into a block whose first instruction is a call (found by the gate:
/// tests/1003_js_gc_cycle.fi).
fn spans(bars: &[usize], calls: &HashMap<usize, Val>, s: usize, e: usize, val: Val) -> bool {
    let mut k = bars.partition_point(|&p| p < s);
    while k < bars.len() && bars[k] <= e {
        if !(bars[k] == s && calls.get(&s) == Some(&val)) {
            return true;
        }
        k += 1;
    }
    false
}

/// The Poletto/Sarkar scan, once for each register class. Returns the register
/// of every interval (or `None` = stays in the frame) and the registers used.
fn linear_scan(
    ivs: &[Iv],
    pool: &[&'static str],
    param_n: usize,
    caller_saved: &[&'static str],
    unscanned: &[&'static str],
) -> (Vec<Option<&'static str>>, Vec<&'static str>) {
    let mut free: Vec<&'static str> = pool.to_vec();
    let mut active: Vec<(usize, &'static str, usize)> = Vec::new(); // (end, reg, index in ivs)
    let mut got: Vec<Option<&'static str>> = vec![None; ivs.len()];
    let mut used: Vec<&'static str> = Vec::new();
    // May interval `k` live in register `r`?
    //  * r841: a parameter must not get a caller-saved register -- the prologue moves the incoming arguments (x0-x7)
    //    one by one and would overwrite a neighbour;
    //  * r845: an interval that spans a call (or another barrier) must not get a caller-saved register, and one that
    //    spans a point where the world may stop must not get a register the collector does not read.
    let fits = |k: usize, r: &str| -> bool {
        let iv = &ivs[k];
        if caller_saved.contains(&r) && (iv.cross || (iv.val as usize) < param_n) {
            return false;
        }
        if unscanned.contains(&r) && iv.gc {
            return false;
        }
        true
    };

    for k in 0..ivs.len() {
        // expire
        active.retain(|&(e, r, _)| {
            if e < ivs[k].start {
                free.push(r);
                false
            } else {
                true
            }
        });
        let pick = free.iter().rposition(|r| fits(k, r));
        if let Some(r) = pick.map(|at| free.remove(at)) {
            got[k] = Some(r);
            if !used.contains(&r) {
                used.push(r);
            }
            active.push((ivs[k].end, r, k));
            active.sort_by_key(|&(e, _, _)| e);
            continue;
        }
        // full: the interval that ends LAST (and whose register the newcomer may use) is the candidate for the
        // stack -- but only if the newcomer is worth more (uses, weighted by loop depth). Otherwise the newcomer
        // goes to the stack.
        if let Some(ai) = active.iter().rposition(|&(_, lr, _)| fits(k, lr)) {
            let (le, lr, li) = active[ai];
            if le > ivs[k].end && ivs[k].weight > ivs[li].weight {
                got[li] = None;
                got[k] = Some(lr);
                active.remove(ai);
                active.push((ivs[k].end, lr, k));
                active.sort_by_key(|&(e, _, _)| e);
            }
        }
    }
    (got, used)
}

pub(crate) fn allocate(f: &Func) -> RaA64 {
    let mut ra = RaA64::default();
    // A switch for narrowing a bug down -- the same means the x86 side has
    // with `FIRN_RA_STATS`/`FIRN_RA_ROUGH`. Whoever hunts a fault wants to
    // know WHICH of the three steps makes it, without recompiling.
    if std::env::var_os("FIRN_A64_RA_OFF").is_some() {
        return ra;
    }
    if f.interrupt {
        return ra; // see the header: INT_SAVE_A64 does not save x19-x24
    }
    let nv = f.val_types.len();
    let nb = f.blocks.len();
    if nv == 0 || nb == 0 || nv.saturating_mul(nb) > 8_000_000 {
        return ra;
    }
    // The linear numbering of `compute_live` assumes block i has id i — the
    // same precondition `regalloc::allocate` checks.
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return ra;
    }
    // AN `asm` BLOCK ENDS IT. The A64 inline assembler (round
    // ARM-FREESTANDING) lets the programmer name ANY register, x19-x28
    // included, as an operand or in the clobber list -- `core.rs:184` lists
    // them as legal names. A value of ours in such a register would be
    // destroyed without a trace. The x86 path draws the same line
    // (`regalloc.rs::unsupported_basic`); here it is drawn in one place
    // because A64 has only this one door.
    if f.blocks.iter().any(|b| b.insts.iter().any(|i| matches!(i.op, crate::fir::Op::Asm { .. }))) {
        return ra;
    }

    // ---- the constants that get rebuilt --------------------------------
    // ROUND 92 (x86, `immediate_consts`) states the precondition and it holds
    // here word for word: only a value with EXACTLY ONE definition IS the
    // constant its `const` instruction names. After `phi.rs` a value can be
    // written from several blocks.
    let mut defs: HashMap<Val, u32> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                *defs.entry(d).or_insert(0) += 1;
            }
        }
    }
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                if defs.get(&d).copied().unwrap_or(0) != 1 || f.is_secret(d) {
                    continue;
                }
                if matches!(i.ty, FTy::F32 | FTy::F64) {
                    // a float constant that `fmov d, #imm8` (or a zero register) can build at the use
                    let single = i.ty == FTy::F32;
                    let bits = i.ty.truncate(*c) as u64;
                    if fp_imm_text(bits, single).is_some() && std::env::var_os("FIRN_A64_NO_REMAT").is_none() {
                        ra.remat.insert(d, bits as i64);
                    }
                    continue;
                }
                if !integral(i.ty) {
                    continue;
                }
                let val = i.ty.truncate(*c) as i64;
                if cheap_const(val) && std::env::var_os("FIRN_A64_NO_REMAT").is_none() {
                    ra.remat.insert(d, val);
                }
            }
        }
    }

    let live = crate::regalloc::compute_live(f);
    let depth = crate::regalloc::loop_depth(f);
    // THE PROMOTED CELL STAYS OFF, AND THAT IS THE SECOND FINDING OF THIS
    // ROUND -- measured, not feared.
    //
    // A promoted cell is a local that lives its WHOLE function in a
    // callee-saved register (interval [0, last access], the rule of round 90).
    // And `emit_gc_addr` writes each of those registers into the collector's
    // state block at EVERY save point, because the scan is conservative. A
    // dead tree root in such a cell keeps its whole tree alive.
    //
    // Measured on `tests/901_dom_tree_gc.fi` with `--no-opt` (the stage
    // without mem2reg, in which EVERY local is a cell), retained after a
    // collection that has to free everything:
    //
    //     with cells      4600 of 4680 nodes
    //     without cells      0
    //
    // And it is NOT the pointer type: `Gc[T]` is a number in FIR, not an
    // `FTy::Ptr` -- the experiment of excluding only `Ptr` kept the same 4600
    // objects. FIR cannot tell a number that is a pointer from a number at
    // all; that is the price of a conservative collector, not carelessness.
    //
    // On x86 the same mechanism is less noticeable, because there are four
    // register pools and only ONE (rbx/rbp/r12-r15) lands in the state block;
    // everything that crosses no call gets r11/r10/rsi/rdi/rdx and is
    // invisible to the collector. On ARM64 all six registers handed out are
    // in the block.
    //
    // The clean way back is known and is in the report: x25-x28 as the
    // NOT collector-visible half, as soon as the state block grows from six to
    // ten words. Until then cells stay in the frame. `FIRN_A64_CELLS=1`
    // switches them on again for measuring.
    let cells = if std::env::var_os("FIRN_A64_CELLS").is_some() {
        crate::regalloc::promotable_cells(f)
    } else {
        HashMap::new()
    };

    // ---- intervals and weights ----------------------------------------
    let mut start = vec![usize::MAX; nv];
    let mut end = vec![0usize; nv];
    let mut weight = vec![0u64; nv];
    let mut touch = |v: Val, p: usize, w: u64, start: &mut Vec<usize>, end: &mut Vec<usize>, weight: &mut Vec<u64>| {
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
    let mut buf: Vec<Val> = Vec::new();
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
            let uses = buf.clone();
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
        let vv = v as Val;
        if start[v] == usize::MAX || cells.contains_key(&vv) || ra.remat.contains_key(&vv) {
            continue;
        }
        if needs_frame_slot(f, vv) {
            continue;
        }
        match f.val_types.get(v) {
            Some(t) if integral(*t) => {}
            _ => continue,
        }
        ivs.push(Iv { val: vv, start: start[v], end: end[v], weight: weight[v], is_cell: false, cross: false, gc: false });
    }
    for (&c, t) in cells.iter() {
        let cv = c as usize;
        if cv >= nv || start[cv] == usize::MAX || !integral(*t) {
            continue;
        }
        // A cell holds a variable: it has to sit in its register from the
        // start of the function to its last access, because a variable in a
        // loop is read again after the back edge. Same rule round 90 settled
        // on for x86 after measuring the tighter answer and throwing it away.
        ivs.push(Iv {
            val: c,
            start: 0,
            end: end[cv],
            weight: weight[cv].saturating_mul(2).max(1),
            is_cell: true,
            cross: false,
            gc: false,
        });
    }
    // ROUND FREG -- the float intervals. Same weights, same scan, the pool
    // is `d8`-`d15`. `FIRN_A64_NO_FREGS=1` switches it off for bisecting.
    let mut fivs: Vec<Iv> = Vec::new();
    if std::env::var_os("FIRN_A64_NO_FREGS").is_none() {
        for v in 0..nv {
            let vv = v as Val;
            if start[v] == usize::MAX || needs_frame_slot(f, vv) || ra.remat.contains_key(&vv) {
                continue;
            }
            if !matches!(f.val_types.get(v), Some(FTy::F32) | Some(FTy::F64)) {
                continue;
            }
            fivs.push(Iv { val: vv, start: start[v], end: end[v], weight: weight[v], is_cell: false, cross: false, gc: false });
        }
    }
    if ivs.is_empty() && fivs.is_empty() {
        ra.on = !ra.remat.is_empty();
        return ra;
    }
    ivs.sort_by_key(|i| (i.start, i.end, i.val));
    fivs.sort_by_key(|i| (i.start, i.end, i.val));

    // ---- the linear scan ----------------------------------------------
    //
    // Poletto/Sarkar, with the one simplification the ABI hands us: every
    // register in the pool survives every call, so a register fits an
    // interval whenever it is free. No crossing mask, no four pools.
    if std::env::var_os("FIRN_A64_NO_REGS").is_some() {
        ra.on = !ra.remat.is_empty();
        return ra;
    }
    // r841: a function with a loop may use x25-x28 as well (see `A64_XPOOL`). They go to the FRONT of the list: the scan
    // hands out from the back, so the extras are only touched when the six ordinary ones are full.
    // r845: not only a LEAF: the barriers (calls, system calls, ...) are known by position, an interval that spans one
    // gets no caller-saved register, and an interval that spans a point where the world may stop gets no x25-x28
    // (calls of silent functions, see `compute_silent`, are no such point). `FIRN_A64_LEAF_ONLY=1` restores r841.
    let leaf = is_leaf(f);
    let leaf_only = std::env::var_os("FIRN_A64_LEAF_ONLY").is_some();
    let xpool_on = std::env::var_os("FIRN_A64_NO_XPOOL").is_none()
        && depth.iter().any(|&d| d > 0)
        && (leaf || !leaf_only);
    // The caller-saved extras in a function WITH calls (crossing analysis) are OFF by default: the gate found four
    // retention failures (tests 1003/1135/1503 js gc, 2421 gc_chunk_index: "too many objects alive after the
    // collection"; any single one of x2-x8/x17 reproduces it, so it is no clobber but a stale conservative root --
    // cause not found yet). `FIRN_A64_NONLEAF_CALLER=1` switches them on for the investigation.
    // A SILENT function (only plain operations and calls of silent functions, see `compute_silent`) is never on the stack
    // when the collector runs, so it cannot retain anything: it gets the caller-saved extras like a leaf.
    let silent_self = SILENT.with(|s| s.borrow().contains(f.name.as_str()));
    let xcaller_on = std::env::var_os("FIRN_A64_NO_XCALLER").is_none()
        && (leaf || silent_self || std::env::var_os("FIRN_A64_NONLEAF_CALLER").is_some());
    let mut pool: Vec<&'static str> = Vec::new();
    let mut caller: Vec<&'static str> = Vec::new();
    if xpool_on {
        pool.extend(A64_XPOOL.iter().rev().copied());
    }
    pool.extend(A64_POOL.iter().copied());
    if xcaller_on {
        // the end of the list is handed out first: caller-saved registers cost no save and restore
        let skip = std::env::var("FIRN_A64_XSKIP").unwrap_or_default();
        caller.extend(A64_XCALLER.iter().rev().copied().filter(|r| !skip.split(',').any(|x| x == *r)));
        pool.extend(caller.iter().copied());
    }
    if !leaf {
        let mut bar_all: Vec<usize> = Vec::new();
        let mut bar_gc: Vec<usize> = Vec::new();
        let mut bar_call: HashMap<usize, Val> = HashMap::new();
        SILENT.with(|sl| {
            let sl = sl.borrow();
            for (bi, b) in f.blocks.iter().enumerate() {
                for (ii, i) in b.insts.iter().enumerate() {
                    if plain_op(&i.op) {
                        continue;
                    }
                    let p = live.pos[bi][ii];
                    bar_all.push(p);
                    match &i.op {
                        Op::Call { name, .. } => {
                            if let Some(d) = i.dst {
                                if !live.live_in[bi][d as usize] {
                                    bar_call.insert(p, d);
                                }
                            }
                            if !sl.contains(name.as_str()) {
                                bar_gc.push(p);
                            }
                        }
                        Op::CallIndirect { .. } => {
                            if let Some(d) = i.dst {
                                if !live.live_in[bi][d as usize] {
                                    bar_call.insert(p, d);
                                }
                            }
                            bar_gc.push(p);
                        }
                        _ => bar_gc.push(p),
                    }
                }
            }
        });
        bar_all.sort_unstable();
        bar_gc.sort_unstable();
        for iv in ivs.iter_mut() {
            iv.cross = spans(&bar_all, &bar_call, iv.start, iv.end, iv.val);
            iv.gc = spans(&bar_gc, &bar_call, iv.start, iv.end, iv.val);
        }
    }
    let unscanned: &[&'static str] = if xpool_on { &A64_XPOOL } else { &[] };
    let (got, used) = linear_scan(&ivs, &pool, f.params.len(), &caller, unscanned);
    let (fgot, fused) = linear_scan(&fivs, &A64_FPOOL, 0, &[], &[]);

    for (k, iv) in ivs.iter().enumerate() {
        if let Some(r) = got[k] {
            if iv.is_cell {
                let t = cells.get(&iv.val).copied().unwrap_or(FTy::I64);
                ra.cells.insert(iv.val, (r, t));
            } else {
                ra.regs.insert(iv.val, r);
            }
        }
    }
    for (k, iv) in fivs.iter().enumerate() {
        if let Some(r) = fgot[k] {
            let single = f.val_types.get(iv.val as usize) == Some(&FTy::F32);
            ra.fregs.insert(iv.val, (r, single));
        }
    }
    if ra.regs.is_empty() && ra.cells.is_empty() && ra.remat.is_empty() && ra.fregs.is_empty() {
        return ra;
    }
    ra.saved = A64_POOL.iter().chain(A64_XPOOL.iter()).copied().filter(|r| used.contains(r)).collect();
    ra.fsaved = A64_FPOOL.iter().copied().filter(|r| fused.contains(r)).collect();

    // ---- the save points -------------------------------------------------
    // For every `Op::GcAddr { regs: true }`: which register of `saved` holds
    // nothing alive at this position? See the long text at the field
    // `safepoints`.
    if live.converged && !ra.saved.is_empty() {
        // Occupied intervals, sorted by register.
        let mut per_reg: HashMap<&'static str, Vec<(usize, usize)>> = HashMap::new();
        for (k, iv) in ivs.iter().enumerate() {
            if let Some(r) = got[k] {
                per_reg.entry(r).or_default().push((iv.start, iv.end));
            }
        }
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, i) in b.insts.iter().enumerate() {
                let d = match (i.dst, &i.op) {
                    (Some(d), crate::fir::Op::GcAddr { regs: true }) => d,
                    _ => continue,
                };
                let p = live.pos[bi][ii];
                let dead: Vec<&'static str> = ra
                    .saved
                    .iter()
                    .copied()
                    .filter(|r| match per_reg.get(r) {
                        Some(v) => !v.iter().any(|&(s, e)| s <= p && p <= e),
                        None => true,
                    })
                    .collect();
                if !dead.is_empty() {
                    ra.safepoints.insert(d, dead);
                }
            }
        }
    }
    // ---- where a pointer dies ----------------------------------------------
    if live.converged {
        let mut per_reg: HashMap<&'static str, Vec<(usize, usize)>> = HashMap::new();
        for (k, iv) in ivs.iter().enumerate() {
            if let Some(r) = got[k] {
                per_reg.entry(r).or_default().push((iv.start, iv.end));
            }
        }
        // position -> (block, instruction number). Only real instruction
        // positions go in, NOT the position of a block's terminator. That was
        // the mistake of the first attempt, and it made the program HANG: if
        // an interval ends on `block_end`, the value is LIVE-OUT of this
        // block -- it travels on over the edge, often over a back edge to the
        // loop head. A `mov r, xzr` in front of that deletes a live value.
        //
        // Conversely: if the end lies on a real instruction position, the
        // value is neither live-out of this block nor live-in of any later one
        // -- otherwise `touch(block_end)` / `touch(block_start)` would have
        // pushed the end there. Exactly then the clearing is safe.
        let mut place: HashMap<usize, (usize, usize)> = HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for ii in 0..b.insts.len() {
                place.insert(live.pos[bi][ii], (bi, ii));
            }
        }
        for (k, iv) in ivs.iter().enumerate() {
            let r = match got[k] {
                Some(r) => r,
                None => continue,
            };
            if f.val_types.get(iv.val as usize) != Some(&FTy::Ptr) {
                continue;
            }
            // r841: x25-x28 and the caller-saved extras are never scanned in a leaf. r845: in a function with calls
            // x25-x28 can be saved by a callee's prologue (its frame is scanned), so a dead pointer must go there too.
            // The caller-saved ones never reach a frame of a callee.
            if !A64_POOL.contains(&r) && !(A64_XPOOL.contains(&r) && !leaf) {
                continue;
            }
            // If the register is needed again right after, the next writer
            // does the work for free.
            let reused = per_reg
                .get(r)
                .map(|v| v.iter().any(|&(s2, e2)| s2 <= iv.end + 1 && iv.end + 1 <= e2))
                .unwrap_or(false);
            if reused {
                continue;
            }
            if let Some(&(bi, ii)) = place.get(&iv.end) {
                let e = ra.clears.entry((bi, ii)).or_default();
                if !e.contains(&r) {
                    e.push(r);
                }
            }
        }
    }
    ra.on = true;
    ra
}

/// Does this instruction read or write the given value? Used by the emitter
/// to decide whether a `str` into the slot may be skipped.
#[allow(dead_code)]
pub(crate) fn touches(i: &Inst, v: Val) -> bool {
    if i.dst == Some(v) {
        return true;
    }
    let mut b = Vec::new();
    i.op.uses(&mut b);
    b.contains(&v)
}

