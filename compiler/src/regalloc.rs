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
        if !changed || rounds > nb + 4 {
            break;
        }
    }
    Live { block_start, block_end, pos, live_in, live_out }
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
    let mut cand: HashMap<Val, i64> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
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
                // Operand `a` goes through `load_ext` (movsx/movzx) -> no immediate
                Op::Bin(BinOp::Div, a, b2) | Op::Bin(BinOp::Rem, a, b2) => {
                    kill(*a, &mut bad);
                    kill(*b2, &mut bad);
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
    crosses_call: bool,
    /// crosses `copymem`/`secure_zero` (they write `rdi`, `rsi`, `rcx`)
    crosses_memop: bool,
    /// crosses `div`/`rem`/`select` (they write `rdx` respectively `rcx`)
    crosses_divsel: bool,
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
    let depth = loop_depth(f);
    let cells = promotable_cells(f);
    alloc.imms = immediate_consts(f);
    alloc.frame_addr = direct_frame_addrs(f, &alloc.frame);
    for v in alloc.imms.keys() {
        alloc.locs[*v as usize] = Loc::Slot(0);
    }

    // call positions (for `crosses_call`)
    let mut call_pos: Vec<usize> = Vec::new();
    let mut memop_pos: Vec<usize> = Vec::new();
    let mut divsel_pos: Vec<usize> = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            if matches!(i.op, Op::Call { .. } | Op::CallIndirect { .. } | Op::Syscall { .. } | Op::ThreadSpawn { .. }) {
                call_pos.push(live.pos[bi][ii]);
            }
            if matches!(i.op, Op::CopyMem { .. } | Op::SecureZero { .. }) {
                memop_pos.push(live.pos[bi][ii]);
            }
            // Round 49: `Op::AtomicCas` uses `rdx` as a third scratch register
            // (`lock cmpxchg [rcx], rdx`) — exactly like `div`/`rem`/`select`.
            // Without this entry an interval living across it keeps carrying
            // `rdx` and is destroyed. Found in tests/820 (release-fast only).
            if matches!(
                i.op,
                Op::Bin(BinOp::Div | BinOp::Rem, _, _) | Op::Select { .. } | Op::AtomicCas { .. }
            ) {
                divsel_pos.push(live.pos[bi][ii]);
            }
        }
    }

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
            continue;
        }
        if f.is_secret(v as Val) {
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
        let cc = call_pos.iter().any(|&p| s <= p && p <= e);
        let cm = memop_pos.iter().any(|&p| s <= p && p <= e);
        let cd = divsel_pos.iter().any(|&p| s <= p && p <= e);
        ivs.push(Iv {
            val: v as Val,
            start: s,
            end: e,
            weight: weight[v],
            crosses_call: cc,
            crosses_memop: cm,
            crosses_divsel: cd,
        });
    }
    for (&c, _) in cells.iter() {
        let cv = c as usize;
        if start[cv] == usize::MAX {
            continue;
        }
        // The cell has to sit in the register from the start of the function
        // to the last access (its content survives blocks without access).
        let s = 0usize;
        let e = end[cv];
        let cc = call_pos.iter().any(|&p| s <= p && p <= e);
        let cm = memop_pos.iter().any(|&p| s <= p && p <= e);
        let cd = divsel_pos.iter().any(|&p| s <= p && p <= e);
        // Cells are almost always the hottest values: double the weight.
        ivs.push(Iv {
            val: c,
            start: s,
            end: e,
            weight: weight[cv].saturating_mul(2).max(1),
            crosses_call: cc,
            crosses_memop: cm,
            crosses_divsel: cd,
        });
    }
    ivs.sort_by_key(|i| (i.start, i.end, i.val));

    // ---- the linear scan itself ------
    //
    // Four pools, from the most restricted to the freest register. `fits`
    // checks whether a register tolerates the crossings of an interval.
    fn fits(iv: &Iv, r: &str) -> bool {
        if CALLEE_SAVED.contains(&r) {
            return true;
        }
        if iv.crosses_call {
            return false;
        }
        if TEMP_REGS.contains(&r) {
            return true;
        }
        if ARG_SPARE.contains(&r) {
            return !iv.crosses_memop;
        }
        if DIV_SPARE.contains(&r) {
            return !iv.crosses_memop && !iv.crosses_divsel;
        }
        false
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
        // minimally tighter; the measurement is in docs/RUNDE49.md §3.
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
        let pick = if !iv.crosses_call {
            if !free_temp.is_empty() {
                free_temp.pop()
            } else if !iv.crosses_memop && !iv.crosses_divsel && !free_div.is_empty() {
                free_div.pop()
            } else if !iv.crosses_memop && !free_arg.is_empty() {
                free_arg.pop()
            } else if !free_saved.is_empty() {
                free_saved.pop()
            } else {
                None
            }
        } else if !free_saved.is_empty() {
            free_saved.pop()
        } else {
            None
        };
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
                        continue;
                    }
                }
                // otherwise this value stays in the stack slot
            }
        }
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
/// miscompile in round 40/41 (docs/RUNDE41.md). Folding happens only when
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
    // The function is emitted into a buffer of its own first; after that the
    // register descriptor post pass strikes spill stores with an immediate
    // reload of the same value (445x statically in the tokenizer run, round 37).
    let mut tmp = Emitter { out: String::new() };
    match emit_with(&mut tmp, f, &a) {
        Ok(()) => {
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
//  * Every other instruction that writes a tracked register as its target
//    operand (mov/lea/add/.../cmov) invalidates exactly that register.
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
    // (docs/RUNDE43.md §6).
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
        // Instructions that write their first operand register.
        if matches!(
            mn,
            "mov" | "movzx" | "movsx" | "movsxd" | "lea" | "add" | "sub" | "and" | "or" | "xor"
                | "imul" | "shl" | "sar" | "shr" | "neg" | "not" | "pop"
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
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Are instruction exact debug lines active (`--no-opt`, see `dwarf.rs`)?
/// Then the base path takes over, so that `.loc` per instruction survives.
fn debug_lines_active(f: &Func) -> bool {
    f.blocks.iter().any(|b| {
        (0..b.insts.len()).any(|i| crate::dwarf::line_at(&f.name, b.id, i as u32).is_some())
    })
}

fn supported(f: &Func) -> bool {
    let basic = unsupported_basic(f);
    if let Some(g) = basic {
        if std::env::var_os("FIRN_RA_WARN").is_some() {
            eprintln!("RA base path: {} — {}", f.name, g);
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
    if debug_lines_active(f) {
        return Some("debug lines active".into());
    }
    // FLOATING POINT: this allocator knows only the integer registers. `f64`
    // lives in the SSE registers and needs a second register class with
    // intervals of its own. As long as that is missing, a function containing
    // `f64` goes over the base path in `codegen_x86.rs` — correct, but without
    // register allocation. Stated honestly in SPEC §14.1.f64.
    if f.val_types.iter().any(|t| *t == FTy::F64) {
        return Some("f64 in the value set".into());
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
                Op::Call { .. } | Op::CallIndirect { .. } | Op::VtabAddr { .. } => {}
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
                // provably right. Stated honestly in docs/RUNDE52.md.
                Op::Asm { .. } => return Some("Inline-Assembler".into()),
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
    e.raw("");
    // Linker symbol through the one spot (codegen_x86::label -> modules::symbol)
    e.raw(&format!(".globl {}", label(&f.name)));
    e.raw(&format!("{}:", label(&f.name)));
    // Line of the `fn` declaration for the debugger (dwarf.rs).
    if let Some((file, line)) = crate::dwarf::fn_line(&f.name) {
        e.line(&format!(".loc {} {} 0", file + 1, line));
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
    for (k, &bi) in order.iter().enumerate() {
        let b = &f.blocks[bi];
        e.raw(&format!("{}:", block_label(&f.name, b.id)));
        // Fallthrough: if the jump target sits right behind it, the jump
        // disappears (saves one `jmp` per BrCond with else==next block).
        let next = order.get(k + 1).map(|&j| f.blocks[j].id);
        emit_block(e, &ra, b, next)?;
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
                    if el < n && !placed[el] {
                        Some(el)
                    } else {
                        Some(*then_bb as usize)
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
        while free < n && placed[free] {
            free += 1;
        }
        if free >= n {
            break;
        }
        b = free;
    }
    out
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

fn emit_block(e: &mut Emitter, ra: &Ra, b: &Block, next: Option<BlockId>) -> Result<(), String> {
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
        emit_inst(e, ra, i)?;
    }
    if mergeable {
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

fn emit_inst(e: &mut Emitter, ra: &Ra, i: &Inst) -> Result<(), String> {
    let ty = i.ty;
    match &i.op {
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
        let mut e = Emitter { out: String::new() };
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
        let mut e = Emitter { out: String::new() };
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

}
