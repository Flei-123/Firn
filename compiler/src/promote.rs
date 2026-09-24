// SPDX-License-Identifier: MPL-2.0
//! **Loop rotation and scalar promotion of memory cells** (round OPT-GENERAL).
//!
//! INTERFACE:
//!   `pub(crate) fn nounmap_functions(m: &Module) -> HashSet<String>`
//!   `pub(crate) fn rotate_loops(f: &mut Func) -> usize`
//!   `pub(crate) fn promote_cells(f: &mut Func, nounmap: &HashSet<String>) -> usize`
//!
//! ## Why
//!
//! Measured on the logic simulator of `bench/simcmp` (sim.fi): the tick loop
//! keeps its state in a struct behind a pointer (`(*s).tick`, `read_q`,
//! `write_n`, ...). Every tick loaded eight fields and stored six, although
//! an idle circuit does nothing else. Kept in locals by hand the same loop ran
//! 1.6x faster natively and in WebAssembly. Keeping only the loads out and the
//! stores in gained NOTHING: the stores have to leave the loop as well.
//!
//! FIR has no alias analysis. The loop also stores octets through addresses
//! computed from integers (`st8(queued, l, 0)`), and nothing proves that such
//! a store cannot hit `(*s).tick`. What CAN be proven is where those stores
//! are: inside the inner loops. So a cell is kept in a register across the
//! outer loop, and handed back to memory exactly around the inner loops that
//! may touch it -- written back before such an inner loop starts, read again
//! after it ends. An inner loop that does not run costs nothing, and that is
//! the idle case.
//!
//! ## Rotation
//!
//! A `while` loop tests at the top: `P -> H(test) -> body -> H`. Its
//! preheader `P` runs even when the loop runs zero times, so nothing may be
//! loaded there speculatively, and code placed in front of an inner loop runs
//! whether that loop iterates or not. `rotate_loops` turns the loop into
//! `P -> G(test) -> P2 -> body -> H(test) -> body`: the test is duplicated into
//! a guard. `P2` runs only when the body runs at least once. The dynamic
//! sequence of instructions is unchanged -- the test runs n+1 times before
//! and after -- which is why any instruction of the header may be duplicated
//! (apart from `alloca` and inline assembler, whose labels would clash).
//!
//! Values of the header used elsewhere are routed through a stack slot first
//! and the next `mem2reg` round rebuilds SSA form with the phis this needs --
//! the classic reg2mem/mem2reg repair, cheap and hard to get wrong.
//!
//! ## Promotion: when a cell may live in a register
//!
//! A CELL is `(root, constant offset, type)`: the address of a load or store,
//! decomposed through `ptradd`/`add` with constants. Two accesses with the
//! same root and disjoint byte ranges never alias. Everything else may alias,
//! except a stack slot whose address never escapes (it can only be reached
//! through its own root).
//!
//! A cell `k` is kept in a register across loop `L` (header `X`, preheader
//! `PH` ending in `br X`) when ALL of this holds:
//!
//!  1. The root is defined outside `L`; the type is a scalar.
//!  2. `L` contains nothing that may unmap memory: no system call, inline
//!     assembler, thread spawn, indirect call, or call of a function that
//!     may do one of these (`nounmap_functions`). That is what makes a load
//!     at `PH` and after an inner loop as safe as the loads of the program.
//!  3. Every instruction of `L` that may WRITE the cell -- other than an exact
//!     store of it -- lies inside a direct inner loop `C` of `L`. Such a `C`
//!     is a REGION for `k`: inside it the cell lives in memory. A region
//!     needs a preheader of its own, and `k` is written back there; every
//!     edge out of the region into `L` gets a block that loads `k` again.
//!     An inner loop that only READS memory which may be `k` is a region too
//!     (written back once instead of once per iteration).
//!  4. Some exact access of `k` outside the regions stands in a block that
//!     dominates every latch and every exiting block of `L`: every iteration
//!     touches the cell, so loading it at `PH` touches nothing the program
//!     would not touch anyway.
//!  5. If `k` is stored outside the regions, some exact STORE dominates every
//!     latch and exiting block as well. The write-back in front of a region
//!     may then happen before the store of the first iteration -- it writes
//!     the very value memory already holds, into a cell the iteration is
//!     going to write anyway.
//!  6. Other accesses with the same root that overlap the cell differently
//!     (another width, a shifted offset) reject the cell.
//!
//! A read of possibly the same memory outside the regions (a load through an
//! unknown address) is allowed: the cell is written back in front of it if it
//! may be dirty.
//!
//! "Dirty" is a forward data flow over the loop: an exact store makes the cell
//! dirty, a write-back or a reload makes it clean, a merge is dirty if any
//! input is. Leaving `L` from a dirty point writes the cell back on the edge.
//!
//! Threads: like every compiler that keeps memory in registers this assumes
//! that no other thread writes the cell while the loop runs without
//! synchronisation. Every atomic operation counts as a write of every cell.

use crate::fir::{BinOp, BlockId, FTy, Func, Inst, Loc, Module, Op, Term, Val};
use std::collections::{HashMap, HashSet};

/// Largest header (instructions besides phis) that `rotate_loops` duplicates.
const ROT_LIMIT: usize = 24;
/// Functions larger than this are left alone (compile time).
const MAX_BLOCKS: usize = 1500;
/// At most this many cells per loop.
const MAX_CELLS: usize = 24;

// ------------------------------------------------------------ helpers ---

/// Visits every operand of an instruction, including the operands the
/// optimizer usually treats as untouchable (a clone needs them renamed too).
pub(crate) fn for_each_use_mut(op: &mut Op, mut g: impl FnMut(&mut Val)) {
    match op {
        Op::Const(_)
        | Op::Alloca { .. }
        | Op::GcAddr { .. }
        | Op::VtabAddr { .. }
        | Op::FnRef { .. }
        | Op::GlobalAddr { .. }
        | Op::ThreadSelf => {}
        Op::Bin(_, a, b) => {
            g(a);
            g(b);
        }
        Op::BinWrapSat { a, b, .. } | Op::CheckedBin { a, b, .. } | Op::CheckedDiv { a, b, .. } => {
            g(a);
            g(b);
        }
        Op::CheckedCast { src, .. } => g(src),
        Op::CheckedIdx { idx, .. } => g(idx),
        Op::Cmp { a, b, .. } => {
            g(a);
            g(b);
        }
        Op::Un(_, a) => g(a),
        Op::Cast { src, .. } => g(src),
        Op::Load { addr } => g(addr),
        Op::Store { addr, val } => {
            g(addr);
            g(val);
        }
        Op::PtrAdd { base, off } => {
            g(base);
            g(off);
        }
        Op::Call { args, .. } | Op::Syscall { args } => {
            for a in args.iter_mut() {
                g(a);
            }
        }
        Op::CallIndirect { target, args } => {
            g(target);
            for a in args.iter_mut() {
                g(a);
            }
        }
        Op::CopyMem { dst, src, .. } => {
            g(dst);
            g(src);
        }
        Op::Select { cond, a, b } => {
            g(cond);
            g(a);
            g(b);
        }
        Op::Barrier { val } => g(val),
        Op::SecureZero { addr, size } => {
            g(addr);
            g(size);
        }
        Op::AtomicAdd { addr, val } => {
            g(addr);
            g(val);
        }
        Op::AtomicCas { addr, erw, new } => {
            g(addr);
            g(erw);
            g(new);
        }
        Op::ThreadSpawn { arg, stack, ctid } => {
            g(arg);
            g(stack);
            g(ctid);
        }
        Op::Asm { ins, outs, .. } => {
            for a in ins.iter_mut() {
                g(a);
            }
            for a in outs.iter_mut() {
                g(a);
            }
        }
        Op::MmioLoad { addr } => g(addr),
        Op::MmioStore { addr, val } => {
            g(addr);
            g(val);
        }
        Op::Simd { args, .. } => {
            for a in args.iter_mut() {
                g(a);
            }
        }
        Op::Phi { incoming } => {
            for (_, v) in incoming.iter_mut() {
                g(v);
            }
        }
        Op::Copy { src } => g(src),
    }
}

fn term_use_mut(t: &mut Term) -> Option<&mut Val> {
    match t {
        Term::BrCond { cond, .. } => Some(cond),
        Term::Switch { val, .. } => Some(val),
        Term::Ret(Some(v)) => Some(v),
        _ => None,
    }
}

fn term_use(t: &Term) -> Option<Val> {
    match t {
        Term::BrCond { cond, .. } => Some(*cond),
        Term::Switch { val, .. } => Some(*val),
        Term::Ret(Some(v)) => Some(*v),
        _ => None,
    }
}

/// Every edge `from -> old` of the terminator becomes `from -> new`.
fn retarget(t: &mut Term, old: BlockId, new: BlockId) {
    match t {
        Term::Br(b) => {
            if *b == old {
                *b = new;
            }
        }
        Term::BrCond { then_bb, else_bb, .. } => {
            if *then_bb == old {
                *then_bb = new;
            }
            if *else_bb == old {
                *else_bb = new;
            }
        }
        Term::Switch { cases, default, .. } => {
            for (_, b) in cases.iter_mut() {
                if *b == old {
                    *b = new;
                }
            }
            if *default == old {
                *default = new;
            }
        }
        Term::Ret(_) | Term::Unset => {}
    }
}

/// In the phis of block `b`: the entry of predecessor `old` now comes from `new`.
fn rename_phi_pred(f: &mut Func, b: usize, old: BlockId, new: BlockId) {
    for i in f.blocks[b].insts.iter_mut() {
        if let Op::Phi { incoming } = &mut i.op {
            for e in incoming.iter_mut() {
                if e.0 == old {
                    e.0 = new;
                }
            }
            incoming.sort_by_key(|e| e.0);
        } else {
            break;
        }
    }
}

/// Puts a fresh, empty block on the edge `from -> to`. Yields its number.
fn split_edge(f: &mut Func, from: usize, to: usize) -> usize {
    let z = f.add_block() as usize;
    f.blocks[z].term = Term::Br(to as BlockId);
    retarget(&mut f.blocks[from].term, to as BlockId, z as BlockId);
    rename_phi_pred(f, to, from as BlockId, z as BlockId);
    z
}

fn natural_loop(head: usize, back: usize, preds: &[Vec<usize>], n: usize) -> Vec<bool> {
    let mut body = vec![false; n];
    body[head] = true;
    let mut stack = Vec::new();
    if !body[back] {
        body[back] = true;
        stack.push(back);
    }
    while let Some(b) = stack.pop() {
        for &p in &preds[b] {
            if !body[p] {
                body[p] = true;
                stack.push(p);
            }
        }
    }
    body
}

struct LoopInfo {
    head: usize,
    body: Vec<bool>,
    size: usize,
    latches: Vec<usize>,
}

impl LoopInfo {
    fn has(&self, b: usize) -> bool {
        b < self.body.len() && self.body[b]
    }
}

/// All natural loops, one per header (bodies of several back edges into the
/// same header are united). Sorted by size, smallest first.
fn find_loops(f: &Func, preds: &[Vec<usize>], dom: &[Vec<bool>]) -> Vec<LoopInfo> {
    let n = f.blocks.len();
    let mut by_head: HashMap<usize, (Vec<bool>, Vec<usize>)> = HashMap::new();
    for (b, blk) in f.blocks.iter().enumerate() {
        for s in blk.term.successors() {
            let h = s as usize;
            if h < n && dom[b][h] {
                let body = natural_loop(h, b, preds, n);
                let e = by_head.entry(h).or_insert_with(|| (vec![false; n], Vec::new()));
                for i in 0..n {
                    e.0[i] |= body[i];
                }
                if !e.1.contains(&b) {
                    e.1.push(b);
                }
            }
        }
    }
    let mut out: Vec<LoopInfo> = by_head
        .into_iter()
        .map(|(head, (body, latches))| {
            let size = body.iter().filter(|x| **x).count();
            LoopInfo { head, body, size, latches }
        })
        .collect();
    out.sort_by_key(|l| (l.size, l.head));
    out
}

fn is_backward_free(f: &Func) -> bool {
    !f.blocks
        .iter()
        .enumerate()
        .any(|(b, blk)| blk.term.successors().into_iter().any(|s| (s as usize) <= b))
}

fn fit(f: &Func) -> bool {
    !f.constant_time
        && !f.interrupt
        && f.secret.is_empty()
        && f.blocks.len() <= MAX_BLOCKS
        && f.blocks.iter().enumerate().all(|(i, b)| b.id as usize == i)
        && !is_backward_free(f)
}

// ----------------------------------------------------------- rotation ---

/// Rotates every `while` shaped loop into a guarded `do while`. Yields the
/// number of rotated loops. Needs a `mem2reg` round afterwards.
pub(crate) fn rotate_loops(f: &mut Func, only: &HashSet<usize>, guards: &HashSet<usize>) -> usize {
    if !fit(f) {
        return 0;
    }
    let mut done = 0;
    let mut tried: HashSet<usize> = HashSet::new();
    // One loop per round; the analysis is rebuilt after every change.
    for _ in 0..64 {
        let preds = crate::mem2reg::preds(f);
        let dom = crate::mem2reg::dominators(f);
        let loops = find_loops(f, &preds, &dom);
        let mut hit = false;
        for l in &loops {
            let all = std::env::var_os("FIRN_PROMOTE_ROTATE_ALL").is_some();
            let full = all || only.contains(&l.head);
            if tried.contains(&l.head) || (!full && !guards.contains(&l.head)) {
                continue;
            }
            tried.insert(l.head);
            if rotate_one(f, l, &preds, !full) {
                done += 1;
                hit = true;
                break;
            }
        }
        if !hit {
            break;
        }
    }
    done
}

/// `guard == false`: full rotation (the test moves to the bottom).
/// `guard == true`: only a GUARD in front -- `P -> G(test) -> P2 -> H(test)`,
/// the loop keeps testing at the top. That is what inner loops get: the
/// preheader P2 still runs only when the body runs (the place for a
/// write-back), and the loop keeps the shape V8's TurboFan unrolls (measured:
/// a fully rotated inner loop cost chain-1000 six percent in Chromium, the
/// guarded one nothing). The header then runs once more than before, so it
/// may only hold instructions without side effects.
fn rotate_one(f: &mut Func, l: &LoopInfo, preds: &[Vec<usize>], guard: bool) -> bool {
    let h = l.head;
    let (cond, then_bb, else_bb) = match f.blocks[h].term {
        Term::BrCond { cond, then_bb, else_bb } => (cond, then_bb as usize, else_bb as usize),
        _ => return false,
    };
    if then_bb == else_bb {
        return false;
    }
    let (x, e) = if l.has(then_bb) && !l.has(else_bb) {
        (then_bb, else_bb)
    } else if l.has(else_bb) && !l.has(then_bb) {
        (else_bb, then_bb)
    } else {
        return false;
    };
    if x == h {
        return false; // already tests at the bottom
    }
    let outer: Vec<usize> = preds[h].iter().copied().filter(|p| !l.has(*p)).collect();
    if outer.len() != 1 {
        return false;
    }
    let p = outer[0];
    if !matches!(f.blocks[p].term, Term::Br(t) if t as usize == h) {
        return false;
    }
    let np = f.blocks[h].phi_count();
    let body_insts = f.blocks[h].insts.len() - np;
    if body_insts > ROT_LIMIT {
        return false;
    }
    for i in &f.blocks[h].insts[np..] {
        if matches!(i.op, Op::Alloca { .. } | Op::Asm { .. } | Op::Phi { .. }) {
            return false;
        }
        if guard {
            let again = matches!(
                i.op,
                Op::Const(_)
                    | Op::Bin(..)
                    | Op::BinWrapSat { .. }
                    | Op::Cmp { .. }
                    | Op::Un(..)
                    | Op::Cast { .. }
                    | Op::PtrAdd { .. }
                    | Op::Load { .. }
                    | Op::Select { .. }
                    | Op::GlobalAddr { .. }
                    | Op::FnRef { .. }
                    | Op::VtabAddr { .. }
                    | Op::CheckedBin { .. }
                    | Op::CheckedDiv { .. }
                    | Op::CheckedCast { .. }
                    | Op::CheckedIdx { .. }
            );
            if !again {
                return false;
            }
        }
    }
    // Values defined in H.
    let mut defined: HashSet<Val> = HashSet::new();
    for i in &f.blocks[h].insts {
        if let Some(d) = i.dst {
            defined.insert(d);
        }
    }
    // Uses of those values outside H (a phi operand counts as a use at the
    // end of its predecessor; an entry for the edge out of H itself is
    // handled by cloning).
    let mut need: Vec<Val> = Vec::new();
    let mut untouchable_use = false;
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            let mut us = Vec::new();
            match &i.op {
                Op::Phi { incoming } => {
                    for (pb, v) in incoming {
                        // guard mode: H still dominates the whole loop, so
                        // only reads from outside it need the slot
                        if *pb as usize != h && defined.contains(v) && (!guard || !l.has(*pb as usize)) {
                            us.push(*v);
                        }
                    }
                }
                other => {
                    if bi != h && (!guard || !l.has(bi)) {
                        other.uses(&mut us);
                        us.retain(|v| defined.contains(v));
                        if !us.is_empty() && crate::mem2reg::is_untouchable(other) {
                            untouchable_use = true;
                        }
                    }
                }
            }
            for v in us {
                if !need.contains(&v) {
                    need.push(v);
                }
            }
        }
        if bi != h && (!guard || !l.has(bi)) {
            if let Some(v) = term_use(&b.term) {
                if defined.contains(&v) && !need.contains(&v) {
                    need.push(v);
                }
            }
        }
    }
    if untouchable_use {
        return false;
    }
    for v in &need {
        let t = f.val_ty(*v);
        if matches!(t, FTy::Void | FTy::V128) {
            return false;
        }
    }
    // The in-loop successor must have H as its only predecessor.
    let x = if !guard && preds[x].len() != 1 { split_edge(f, h, x) } else { x };

    // ---- demote the values used outside H into slots
    let mut slot_of: HashMap<Val, Val> = HashMap::new();
    for v in &need {
        let t = f.val_ty(*v);
        let bytes = t.bytes().max(1);
        let s = f.alloca(bytes, bytes.min(8));
        slot_of.insert(*v, s);
    }
    if !need.is_empty() {
        // stores right after the definitions (after the phi prefix for phis)
        let old = std::mem::take(&mut f.blocks[h].insts);
        let np = old.iter().take_while(|i| matches!(i.op, Op::Phi { .. })).count();
        let mut new: Vec<Inst> = Vec::with_capacity(old.len() + need.len());
        let mut phi_stores: Vec<Inst> = Vec::new();
        for (ix, i) in old.into_iter().enumerate() {
            let d = i.dst;
            let loc = i.loc;
            let ty = i.ty;
            new.push(i);
            if let Some(d) = d {
                if let Some(&s) = slot_of.get(&d) {
                    let st = Inst::like(None, ty, Op::Store { addr: s, val: d }, loc);
                    if ix < np {
                        phi_stores.push(st);
                    } else {
                        new.push(st);
                    }
                }
            }
            if ix + 1 == np {
                new.append(&mut phi_stores);
            }
        }
        if np == new.len() {
            // cannot happen (np counted from `old`), kept for clarity
        }
        f.blocks[h].insts = new;
        // uses outside H become loads
        let nb = f.blocks.len();
        for bi in 0..nb {
            // phi operands -> load at the end of the predecessor. This
            // includes the phis of H ITSELF: an entry from a latch that
            // names a value of H (`in_class` unchanged on a `continue`,
            // the phi naming itself) is read at the end of that latch, and
            // after rotation the first pass reaches the latch without ever
            // running H -- the value has to come from the slot the guard
            // wrote.
            let mut tail_loads: Vec<(usize, Val, Val)> = Vec::new(); // (pred, old, slot)
            {
                let blk = &f.blocks[bi];
                for i in blk.insts.iter() {
                    if let Op::Phi { incoming } = &i.op {
                        for (pb, v) in incoming {
                            if *pb as usize != h && (!guard || !l.has(*pb as usize)) {
                                if let Some(&s) = slot_of.get(v) {
                                    tail_loads.push((*pb as usize, *v, s));
                                }
                            }
                        }
                    } else {
                        break;
                    }
                }
            }
            for (pb, v, s) in tail_loads {
                let t = f.val_ty(v);
                let nv = f.new_val_pub(t);
                f.blocks[pb].insts.push(Inst::new(Some(nv), t, Op::Load { addr: s }));
                for i in f.blocks[bi].insts.iter_mut() {
                    if let Op::Phi { incoming } = &mut i.op {
                        for e in incoming.iter_mut() {
                            if e.0 as usize == pb && e.1 == v {
                                e.1 = nv;
                            }
                        }
                    } else {
                        break;
                    }
                }
            }
            if bi == h || (guard && l.has(bi)) {
                continue; // ordinary uses inside H (the loop) read the value directly
            }
            // ordinary uses -> load right in front
            let old = std::mem::take(&mut f.blocks[bi].insts);
            let mut new: Vec<Inst> = Vec::with_capacity(old.len());
            for mut i in old.into_iter() {
                if !matches!(i.op, Op::Phi { .. }) {
                    let mut us = Vec::new();
                    i.op.uses(&mut us);
                    let mut local: HashMap<Val, Val> = HashMap::new();
                    for u in us {
                        if let Some(&s) = slot_of.get(&u) {
                            if !local.contains_key(&u) {
                                let t = f.val_ty(u);
                                let nv = f.new_val_pub(t);
                                new.push(Inst::like(Some(nv), t, Op::Load { addr: s }, i.loc));
                                local.insert(u, nv);
                            }
                        }
                    }
                    if !local.is_empty() {
                        for_each_use_mut(&mut i.op, |v| {
                            if let Some(&nv) = local.get(v) {
                                *v = nv;
                            }
                        });
                    }
                }
                new.push(i);
            }
            f.blocks[bi].insts = new;
            let tu = term_use(&f.blocks[bi].term);
            if let Some(u) = tu {
                if let Some(&s) = slot_of.get(&u) {
                    let t = f.val_ty(u);
                    let nv = f.new_val_pub(t);
                    f.blocks[bi].insts.push(Inst::new(Some(nv), t, Op::Load { addr: s }));
                    if let Some(r) = term_use_mut(&mut f.blocks[bi].term) {
                        *r = nv;
                    }
                }
            }
        }
    }

    // ---- the guard: a clone of H for the entry from P
    let g = f.add_block() as usize;
    let p2 = f.add_block() as usize;
    let mut map: HashMap<Val, Val> = HashMap::new();
    let hinsts = f.blocks[h].insts.clone();
    let mut ginsts: Vec<Inst> = Vec::with_capacity(hinsts.len());
    for i in hinsts {
        match &i.op {
            Op::Phi { incoming } => {
                let v = match incoming.iter().find(|(b, _)| *b as usize == p) {
                    Some((_, v)) => *v,
                    None => return false, // malformed phi; cannot happen after verify
                };
                if let Some(d) = i.dst {
                    map.insert(d, v);
                }
            }
            _ => {
                let mut c = i.clone();
                for_each_use_mut(&mut c.op, |v| {
                    if let Some(&nv) = map.get(v) {
                        *v = nv;
                    }
                });
                if let Some(d) = c.dst {
                    let nd = f.new_val_pub(c.ty);
                    map.insert(d, nd);
                    c.dst = Some(nd);
                }
                ginsts.push(c);
            }
        }
    }
    let gcond = *map.get(&cond).unwrap_or(&cond);
    f.blocks[g].insts = ginsts;
    f.blocks[g].term = if then_bb == e {
        Term::BrCond { cond: gcond, then_bb: e as BlockId, else_bb: p2 as BlockId }
    } else {
        Term::BrCond { cond: gcond, then_bb: p2 as BlockId, else_bb: e as BlockId }
    };
    retarget(&mut f.blocks[p].term, h as BlockId, g as BlockId);
    if guard {
        // P2 enters H in place of P: the entries of H stay, renamed
        f.blocks[p2].term = Term::Br(h as BlockId);
        rename_phi_pred(f, h, p as BlockId, p2 as BlockId);
    } else {
        f.blocks[p2].term = Term::Br(x as BlockId);
        // H loses the entry from P.
        for i in f.blocks[h].insts.iter_mut() {
            if let Op::Phi { incoming } = &mut i.op {
                incoming.retain(|(b, _)| *b as usize != p);
            } else {
                break;
            }
        }
    }
    // X (rotation only) and E gain an entry for the new edge.
    let edges: Vec<(usize, usize)> = if guard { vec![(e, g)] } else { vec![(x, p2), (e, g)] };
    for (succ, from) in edges {
        for i in f.blocks[succ].insts.iter_mut() {
            if let Op::Phi { incoming } = &mut i.op {
                if let Some(v) = incoming.iter().find(|(b, _)| *b as usize == h).map(|(_, v)| *v) {
                    let nv = *map.get(&v).unwrap_or(&v);
                    incoming.push((from as BlockId, nv));
                    incoming.sort_by_key(|e| e.0);
                }
            } else {
                break;
            }
        }
    }
    true
}

// ------------------------------------------------------ side effects ---

/// System calls (canonical x86-64 numbers, which FIR carries on every
/// target) that can take memory away or share it with another thread of
/// control: munmap, mremap, brk, mprotect, madvise, clone, fork, vfork,
/// execve, shmdt, execveat, clone3. Every other call is an ordinary
/// read/write of memory for this pass. An unknown number is the worst case.
fn syscall_unmaps(nr: Option<i128>) -> bool {
    match nr {
        Some(n) => matches!(n, 10 | 11 | 12 | 25 | 28 | 56 | 57 | 58 | 59 | 67 | 322 | 435),
        None => true,
    }
}

fn const_defs(f: &Func) -> HashMap<Val, i128> {
    let mut cnt: HashMap<Val, u32> = HashMap::new();
    let mut val: HashMap<Val, i128> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                *cnt.entry(d).or_insert(0) += 1;
                if let Op::Const(c) = &i.op {
                    val.insert(d, *c);
                }
            }
        }
    }
    val.retain(|k, _| cnt.get(k).copied() == Some(1));
    val
}

/// Functions that cannot unmap memory: no system call that may (see
/// `syscall_unmaps`), no inline assembler, no thread, no indirect call, and
/// only calls of such functions. Greatest fixed point (recursion is fine).
pub(crate) fn nounmap_functions(m: &Module) -> HashSet<String> {
    // per function: does it do something bad itself, and whom does it call
    let idx: HashMap<&str, usize> = m.funcs.iter().enumerate().map(|(i, f)| (f.name.as_str(), i)).collect();
    let n = m.funcs.len();
    let mut bad = vec![false; n];
    let mut callers: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (fi, f) in m.funcs.iter().enumerate() {
        let consts = const_defs(f);
        for b in &f.blocks {
            for i in &b.insts {
                match &i.op {
                    Op::Syscall { args } => {
                        if syscall_unmaps(args.first().and_then(|a| consts.get(a).copied())) {
                            bad[fi] = true;
                        }
                    }
                    Op::Asm { .. } | Op::ThreadSpawn { .. } | Op::CallIndirect { .. } => bad[fi] = true,
                    Op::Call { name, .. } => match idx.get(name.as_str()) {
                        Some(&c) => callers[c].push(fi),
                        None => bad[fi] = true,
                    },
                    _ => {}
                }
            }
        }
    }
    // everything that reaches a bad function is bad
    let mut work: Vec<usize> = (0..n).filter(|i| bad[*i]).collect();
    while let Some(c) = work.pop() {
        for &k in &callers[c] {
            if !bad[k] {
                bad[k] = true;
                work.push(k);
            }
        }
    }
    m.funcs.iter().enumerate().filter(|(i, _)| !bad[*i]).map(|(_, f)| f.name.clone()).collect()
}

/// One memory access with a decomposed address.
#[derive(Clone, Copy, Debug)]
struct Acc {
    root: Val,
    off: Option<i64>,
    len: Option<u64>,
    read: bool,
    write: bool,
}

#[derive(Clone, Debug)]
enum Eff {
    Pure,
    Mem(Vec<Acc>),
    /// may read and/or write any memory that is not a private slot
    Wild { read: bool, write: bool },
    /// may unmap memory
    Unmap,
}

struct Ctx<'a> {
    f: &'a Func,
    def: Vec<Option<(usize, usize)>>,
    private: HashSet<Val>,
    nounmap: &'a HashSet<String>,
    /// per instruction: its effect, and for a load/store the decomposed
    /// address (computed once -- the analysis asks for every cell)
    effs: Vec<Vec<Eff>>,
    addrs: Vec<Vec<Option<(Val, Option<i64>)>>>,
}

impl<'a> Ctx<'a> {
    fn new(f: &'a Func, nounmap: &'a HashSet<String>) -> Ctx<'a> {
        let mut def = vec![None; f.val_types.len()];
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, i) in b.insts.iter().enumerate() {
                if let Some(d) = i.dst {
                    if (d as usize) < def.len() {
                        def[d as usize] = Some((bi, ii));
                    }
                }
            }
        }
        let private = private_slots(f);
        let mut cx = Ctx { f, def, private, nounmap, effs: Vec::new(), addrs: Vec::new() };
        let mut effs = Vec::with_capacity(f.blocks.len());
        let mut addrs = Vec::with_capacity(f.blocks.len());
        for b in &f.blocks {
            effs.push(b.insts.iter().map(|i| cx.eff(i)).collect::<Vec<Eff>>());
            addrs.push(
                b.insts
                    .iter()
                    .map(|i| match &i.op {
                        Op::Load { addr } | Op::Store { addr, .. } => Some(cx.decompose(*addr)),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            );
        }
        cx.effs = effs;
        cx.addrs = addrs;
        cx
    }
    fn op_of(&self, v: Val) -> Option<&Inst> {
        let (b, i) = (*self.def.get(v as usize)?)?;
        self.f.blocks.get(b)?.insts.get(i)
    }
    fn const_of(&self, v: Val) -> Option<i64> {
        match self.op_of(v).map(|i| &i.op) {
            Some(Op::Const(c)) => i64::try_from(*c).ok().or(Some(*c as i64)),
            _ => None,
        }
    }
    fn def_block(&self, v: Val) -> Option<usize> {
        self.def.get(v as usize).copied().flatten().map(|(b, _)| b)
    }
    fn decompose(&self, addr: Val) -> (Val, Option<i64>) {
        let mut v = addr;
        let mut off: i64 = 0;
        let mut known = true;
        for _ in 0..64 {
            let inst = match self.op_of(v) {
                Some(i) => i,
                None => break,
            };
            match &inst.op {
                Op::PtrAdd { base, off: o } => {
                    match self.const_of(*o) {
                        Some(c) => off = off.wrapping_add(c),
                        None => known = false,
                    }
                    v = *base;
                }
                Op::Bin(BinOp::Add, a, b) if inst.ty.bits() == 64 && !inst.ty.is_float() => {
                    if let Some(c) = self.const_of(*b) {
                        off = off.wrapping_add(c);
                        v = *a;
                    } else if let Some(c) = self.const_of(*a) {
                        off = off.wrapping_add(c);
                        v = *b;
                    } else {
                        break;
                    }
                }
                Op::Cast { src, from }
                    if inst.ty.bits() == 64 && from.bits() == 64 && !inst.ty.is_float() && !from.is_float() =>
                {
                    v = *src;
                }
                _ => break,
            }
        }
        (v, if known { Some(off) } else { None })
    }
    fn acc(&self, addr: Val, len: Option<u64>, read: bool, write: bool) -> Acc {
        let (root, off) = self.decompose(addr);
        Acc { root, off, len, read, write }
    }
    fn eff(&self, i: &Inst) -> Eff {
        match &i.op {
            Op::Load { addr } => Eff::Mem(vec![self.acc(*addr, Some(i.ty.bytes()), true, false)]),
            Op::Store { addr, .. } => Eff::Mem(vec![self.acc(*addr, Some(i.ty.bytes()), false, true)]),
            Op::CopyMem { dst, src, size } => Eff::Mem(vec![
                self.acc(*src, Some(*size), true, false),
                self.acc(*dst, Some(*size), false, true),
            ]),
            Op::Simd { kind, args, .. } => match kind {
                crate::simd::SimdKind::Load if !args.is_empty() => {
                    Eff::Mem(vec![self.acc(args[0], Some(16), true, false)])
                }
                crate::simd::SimdKind::Store if !args.is_empty() => {
                    Eff::Mem(vec![self.acc(args[0], Some(16), false, true)])
                }
                k if !k.is_pure() => Eff::Wild { read: true, write: true },
                _ => Eff::Pure,
            },
            Op::AtomicAdd { .. } | Op::AtomicCas { .. } => Eff::Wild { read: true, write: true },
            Op::MmioLoad { .. } | Op::MmioStore { .. } => Eff::Wild { read: true, write: true },
            Op::SecureZero { .. } => Eff::Wild { read: true, write: true },
            Op::GcAddr { regs: true } => Eff::Wild { read: true, write: true },
            Op::Call { name, .. } => {
                if self.nounmap.contains(name) {
                    Eff::Wild { read: true, write: true }
                } else {
                    Eff::Unmap
                }
            }
            Op::Syscall { args } => {
                let nr = args.first().and_then(|a| self.const_of(*a)).map(|c| c as i128);
                if syscall_unmaps(nr) {
                    Eff::Unmap
                } else {
                    Eff::Wild { read: true, write: true }
                }
            }
            Op::CallIndirect { .. } | Op::Asm { .. } | Op::ThreadSpawn { .. } => Eff::Unmap,
            _ => Eff::Pure,
        }
    }
}

/// Stack slots whose address never leaves loads/stores (as the address) and
/// constant `ptradd`s thereof: nothing but their own root can reach them.
fn private_slots(f: &Func) -> HashSet<Val> {
    let mut cand: HashSet<Val> = HashSet::new();
    for i in &f.blocks[0].insts {
        if let (Op::Alloca { .. }, Some(d)) = (&i.op, i.dst) {
            cand.insert(d);
        }
    }
    if cand.is_empty() {
        return cand;
    }
    // derived pointer -> its alloca
    let mut owner: HashMap<Val, Val> = cand.iter().map(|v| (*v, *v)).collect();
    // ptradds may be defined in any block; iterate until stable
    loop {
        let mut grew = false;
        for b in &f.blocks {
            for i in &b.insts {
                if let (Op::PtrAdd { base, .. }, Some(d)) = (&i.op, i.dst) {
                    if let Some(&o) = owner.get(base) {
                        if !owner.contains_key(&d) {
                            owner.insert(d, o);
                            grew = true;
                        }
                    }
                }
            }
        }
        if !grew {
            break;
        }
    }
    let mut bad: HashSet<Val> = HashSet::new();
    let mut us = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                Op::Load { .. } => {}
                Op::Store { val, .. } => {
                    if let Some(&o) = owner.get(val) {
                        bad.insert(o);
                    }
                }
                Op::PtrAdd { off, .. } => {
                    if let Some(&o) = owner.get(off) {
                        bad.insert(o);
                    }
                }
                other => {
                    us.clear();
                    other.uses(&mut us);
                    for u in &us {
                        if let Some(&o) = owner.get(u) {
                            bad.insert(o);
                        }
                    }
                }
            }
        }
        if let Some(v) = term_use(&b.term) {
            if let Some(&o) = owner.get(&v) {
                bad.insert(o);
            }
        }
    }
    // The root of a decomposed address is the alloca itself (decompose walks
    // through the ptradds), so only allocas need to be in the set.
    cand.retain(|v| !bad.contains(v));
    cand
}

// ----------------------------------------------------------- promotion ---

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Cell {
    root: Val,
    off: i64,
    ty: FTy,
}

impl std::hash::Hash for Cell {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.root.hash(h);
        self.off.hash(h);
        self.ty.name().hash(h);
    }
}

fn overlap(a_off: i64, a_len: u64, b_off: i64, b_len: u64) -> bool {
    let (a0, a1) = (a_off as i128, a_off as i128 + a_len as i128);
    let (b0, b1) = (b_off as i128, b_off as i128 + b_len as i128);
    a0 < b1 && b0 < a1
}

impl<'a> Ctx<'a> {
    /// May the access touch cell `k`?
    fn aliases(&self, a: &Acc, k: &Cell) -> bool {
        if a.root == k.root {
            return match (a.off, a.len) {
                (Some(o), Some(l)) => overlap(o, l, k.off, k.ty.bytes()),
                _ => true,
            };
        }
        if self.private.contains(&a.root) || self.private.contains(&k.root) {
            return false;
        }
        true
    }
    /// Some(false) = exact load of `k`, Some(true) = exact store
    fn is_exact(&self, b: usize, ii: usize, k: &Cell) -> Option<bool> {
        let i = &self.f.blocks[b].insts[ii];
        let store = match &i.op {
            Op::Load { .. } => false,
            Op::Store { .. } => true,
            _ => return None,
        };
        if i.ty != k.ty {
            return None;
        }
        match self.addrs[b][ii] {
            Some((r, o)) if r == k.root && o == Some(k.off) => Some(store),
            _ => None,
        }
    }
    /// Does the instruction touch `k` in any way other than an exact access?
    /// Yields (reads, writes).
    fn clobbers(&self, b: usize, ii: usize, k: &Cell) -> (bool, bool) {
        if self.is_exact(b, ii, k).is_some() {
            return (false, false);
        }
        match self.effs[b][ii].clone() {
            Eff::Pure => (false, false),
            Eff::Mem(v) => {
                let mut r = false;
                let mut w = false;
                for a in &v {
                    if self.aliases(a, k) {
                        r |= a.read;
                        w |= a.write;
                    }
                }
                (r, w)
            }
            Eff::Wild { read, write } => {
                if self.private.contains(&k.root) {
                    (false, false)
                } else {
                    (read, write)
                }
            }
            Eff::Unmap => (true, true),
        }
    }
}

/// Cheap pre-check (one dominator computation): is there a loop without
/// anything that may unmap memory, with a load or store of a cell whose root
/// comes from outside the loop? Only then is rotating and promoting worth
/// the compile time -- measured on bin/firnc1.fi, three functions of 1,300
/// pass it.
/// The loops worth rotating (promotion targets) and the inner loops worth a
/// guard (their direct children).
pub(crate) fn candidate_loops(f: &mut Func, nounmap: &HashSet<String>) -> (HashSet<usize>, HashSet<usize>) {
    let mut heads: HashSet<usize> = HashSet::new();
    let mut kids: HashSet<usize> = HashSet::new();
    if !fit(f) {
        return (heads, kids);
    }
    let mut rejected: HashSet<(usize, Cell)> = HashSet::new();
    let mut found: Vec<(usize, Vec<usize>)> = Vec::new();
    scan(f, nounmap, &mut rejected, Some(&mut found));
    let no_children = std::env::var_os("FIRN_PROMOTE_NO_CHILD_GUARD").is_some();
    let rotate_children = std::env::var_os("FIRN_PROMOTE_ROTATE_CHILDREN").is_some();
    for (h, children) in found {
        heads.insert(h);
        if rotate_children {
            heads.extend(children);
        } else if !no_children {
            kids.extend(children);
        }
    }
    for h in &heads {
        kids.remove(h);
    }
    (heads, kids)
}

/// Keeps memory cells in registers across loops (see the module text).
/// Yields the number of promoted cells. Needs a `mem2reg` round afterwards.
pub(crate) fn promote_cells(f: &mut Func, nounmap: &HashSet<String>) -> usize {
    if !fit(f) {
        return 0;
    }
    let mut done = 0;
    let mut rejected: HashSet<(usize, Cell)> = HashSet::new();
    for _ in 0..256 {
        match scan(f, nounmap, &mut rejected, None) {
            true => done += 1,
            false => break,
        }
    }
    done
}

/// Finds and promotes ONE cell (outer loops first). The analysis is rebuilt
/// after every change, which keeps each step simple.
/// With `found == None`: finds and promotes ONE cell. With `Some`: only
/// looks (before rotation, with the two conditions rotation establishes
/// relaxed) and collects every loop that has a cell, with its direct
/// children -- the loops worth rotating.
fn scan(
    f: &mut Func,
    nounmap: &HashSet<String>,
    rejected: &mut HashSet<(usize, Cell)>,
    mut found: Option<&mut Vec<(usize, Vec<usize>)>>,
) -> bool {
    let check = found.is_some();
    let preds = crate::mem2reg::preds(f);
    let dom = crate::mem2reg::dominators(f);
    let loops = find_loops(f, &preds, &dom);
    if loops.is_empty() {
        return false;
    }
    let cx = Ctx::new(f, nounmap);
    // outermost first
    let mut order: Vec<usize> = (0..loops.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(loops[i].size), loops[i].head));
    for li in order {
        let l = &loops[li];
        let x = l.head;
        // preheader: the one outside predecessor, ending in `br X`
        let outer: Vec<usize> = preds[x].iter().copied().filter(|p| !l.has(*p)).collect();
        if outer.len() != 1 {
            continue;
        }
        let ph = outer[0];
        if !check && !matches!(f.blocks[ph].term, Term::Br(t) if t as usize == x) {
            continue;
        }
        let blocks: Vec<usize> = (0..f.blocks.len()).filter(|b| l.has(*b)).collect();
        // (2) nothing that may unmap memory
        let unmap = blocks
            .iter()
            .any(|&b| cx.effs[b].iter().any(|e| matches!(e, Eff::Unmap)));
        if unmap {
            continue;
        }
        // direct children
        let children: Vec<usize> = (0..loops.len())
            .filter(|&c| {
                let lc = &loops[c];
                c != li
                    && lc.head != x
                    && l.has(lc.head)
                    && lc.size < l.size
                    && !(0..loops.len()).any(|m| {
                        m != li
                            && m != c
                            && loops[m].size > lc.size
                            && loops[m].size < l.size
                            && l.has(loops[m].head)
                            && loops[m].has(lc.head)
                    })
            })
            .collect();
        // exiting blocks of L
        let exiting: Vec<usize> = blocks
            .iter()
            .copied()
            .filter(|&b| f.blocks[b].term.successors().iter().any(|s| !l.has(*s as usize)))
            .collect();
        // candidate cells
        let mut cells: Vec<Cell> = Vec::new();
        for &b in &blocks {
            for (ii, i) in f.blocks[b].insts.iter().enumerate() {
                let (root, off) = match cx.addrs[b][ii] {
                    Some(x) => x,
                    None => continue,
                };
                if matches!(i.ty, FTy::V128 | FTy::Void) {
                    continue;
                }
                let off = match off {
                    Some(o) => o,
                    None => continue,
                };
                if let Some(db) = cx.def_block(root) {
                    if l.has(db) {
                        continue;
                    }
                }
                // stack slots are mem2reg's business (and our own slots
                // would be found again)
                if matches!(cx.op_of(root).map(|i| &i.op), Some(Op::Alloca { .. })) {
                    continue;
                }
                let k = Cell { root, off, ty: i.ty };
                if !cells.contains(&k) && !rejected.contains(&(x, k)) {
                    cells.push(k);
                }
            }
        }
        cells.truncate(MAX_CELLS);
        for k in cells {
            match analyse(f, &cx, &preds, &dom, &loops, l, &blocks, &children, &exiting, check, &k) {
                Some(_) if check => {
                    let ch: Vec<usize> = children.iter().map(|&c| loops[c].head).collect();
                    if let Some(v) = found.as_deref_mut() {
                        v.push((x, ch));
                    }
                    break; // one cell is enough to rotate this loop
                }
                Some(plan) => {
                    drop(cx);
                    if std::env::var_os("FIRN_PROMOTE_TRACE").is_some() {
                        eprintln!("promote:   @{} loop bb{} cell root %{} off {} {} ({} regions)",
                                  f.name, x, k.root, k.off, k.ty.name(), plan.regions.len());
                    }
                    apply(f, &plan, &k, l, ph);
                    // never twice: the write-backs and reloads are exact
                    // accesses of the same cell again
                    rejected.insert((x, k));
                    return true;
                }
                None => {
                    rejected.insert((x, k));
                }
            }
        }
    }
    false
}

struct Plan {
    /// blocks of L outside the regions
    live: Vec<bool>,
    /// per region: (preheader, blocks of the region)
    regions: Vec<(usize, Vec<bool>)>,
    /// (block, instruction index) of reads through possibly aliasing addresses
    reads: HashSet<(usize, usize)>,
    /// exact accesses outside the regions: (block, index)
    exact: Vec<(usize, usize)>,
}

#[allow(clippy::too_many_arguments)]
fn analyse(
    f: &Func,
    cx: &Ctx,
    preds: &[Vec<usize>],
    dom: &[Vec<bool>],
    loops: &[LoopInfo],
    l: &LoopInfo,
    blocks: &[usize],
    children: &[usize],
    exiting: &[usize],
    relaxed: bool,
    k: &Cell,
) -> Option<Plan> {
    let n = f.blocks.len();
    // regions: direct children with any clobber of k
    let mut live = vec![false; n];
    for &b in blocks {
        live[b] = true;
    }
    let mut regions: Vec<(usize, Vec<bool>)> = Vec::new();
    for &c in children {
        let lc = &loops[c];
        let mut touched = false;
        'scan: for b in 0..n {
            if !lc.has(b) {
                continue;
            }
            for ii in 0..f.blocks[b].insts.len() {
                let (r, w) = cx.clobbers(b, ii, k);
                if r || w {
                    touched = true;
                    break 'scan;
                }
            }
        }
        if !touched {
            continue;
        }
        // preheader of the region
        let outer: Vec<usize> = preds[lc.head].iter().copied().filter(|p| !lc.has(*p)).collect();
        if outer.len() != 1 {
            return None;
        }
        let pc = outer[0];
        if !relaxed && (!matches!(f.blocks[pc].term, Term::Br(t) if t as usize == lc.head) || !l.has(pc)) {
            return None;
        }
        for b in 0..n {
            if lc.has(b) {
                live[b] = false;
            }
        }
        regions.push((pc, lc.body.clone()));
    }
    // a region's preheader must not lie in another region, and no region may
    // exit straight into another region's header
    for (pc, _) in &regions {
        if !live[*pc] {
            return None;
        }
    }
    for (_, body) in &regions {
        for b in 0..n {
            if !body[b] {
                continue;
            }
            for s in f.blocks[b].term.successors() {
                let s = s as usize;
                if l.has(s) && !body[s] && regions.iter().any(|(_, ob)| ob[s]) {
                    return None;
                }
            }
        }
    }
    // outside the regions: exact accesses, reads, no foreign writes
    let mut exact: Vec<(usize, usize)> = Vec::new();
    let mut reads: HashSet<(usize, usize)> = HashSet::new();
    let mut guard_access = false;
    let mut any_store = false;
    let mut guard_store = false;
    let dominates_all = |d: usize| -> bool {
        l.latches.iter().all(|&t| dom[t][d]) && exiting.iter().all(|&e| dom[e][d])
    };
    for &b in blocks {
        if !live[b] {
            continue;
        }
        let dall = dominates_all(b);
        for (ii, i) in f.blocks[b].insts.iter().enumerate() {
            if let Some(st) = cx.is_exact(b, ii, k) {
                if f.is_secret(i.dst.unwrap_or(u32::MAX)) {
                    return None;
                }
                if let Op::Store { val, .. } = &i.op {
                    if f.is_secret(*val) {
                        return None;
                    }
                }
                exact.push((b, ii));
                if dall {
                    guard_access = true;
                }
                if st {
                    any_store = true;
                    if dall {
                        guard_store = true;
                    }
                }
                continue;
            }
            let (r, w) = cx.clobbers(b, ii, k);
            if w {
                return None;
            }
            if r {
                // an overlapping but different access of the same root is
                // not a harmless read: it would see a stale cell only if
                // dirty, which the write back handles -- but a partial
                // overlap through the SAME root is a sign of type punning,
                // keep away from it
                if let Eff::Mem(v) = &cx.effs[b][ii] {
                    if v.iter().any(|a| a.root == k.root) {
                        return None;
                    }
                }
                reads.insert((b, ii));
            }
        }
    }
    if exact.is_empty() {
        return None;
    }
    // the pre-check (before rotation) cannot ask for these two: rotation is
    // what makes the body's first block dominate the loop exit
    if !relaxed && !guard_access {
        return None;
    }
    if !relaxed && any_store && !guard_store {
        return None;
    }
    // Is it worth it? At least one exact access inside a loop block that
    // runs every iteration -- given by guard_access. Nothing else to check.
    Some(Plan { live, regions, reads, exact })
}

fn addr_expr(f: &mut Func, b: usize, root: Val, off: i64) -> Val {
    let rt = f.val_ty(root);
    if off == 0 {
        return root;
    }
    if rt == FTy::Ptr {
        let c = f.push(b as BlockId, FTy::I64, Op::Const(off as i128));
        f.push(b as BlockId, FTy::Ptr, Op::PtrAdd { base: root, off: c })
    } else {
        let t = if rt.bits() == 64 && !rt.is_float() { rt } else { FTy::U64 };
        let c = f.push(b as BlockId, t, Op::Const(t.truncate(off as i128)));
        f.push(b as BlockId, t, Op::Bin(BinOp::Add, root, c))
    }
}

fn apply(f: &mut Func, plan: &Plan, k: &Cell, l: &LoopInfo, ph: usize) {
    let ty = k.ty;
    let bytes = ty.bytes().max(1);
    let saved_stamp = f.loc_stamp;
    f.loc_stamp = Loc::NONE;
    let slot = f.alloca(bytes, bytes.min(8));
    // The alloca went into block 0; positions recorded for block 0 shift by
    // one. Block 0 can never be inside a loop (it has no predecessor), so
    // nothing recorded is affected -- but the preheader may be block 0,
    // and we only append there.
    let addr = addr_expr(f, ph, k.root, k.off);
    let t0 = f.push(ph as BlockId, ty, Op::Load { addr });
    f.push_void(ph as BlockId, ty, Op::Store { addr: slot, val: t0 });

    // exact accesses -> the slot
    for &(b, ii) in &plan.exact {
        match &mut f.blocks[b].insts[ii].op {
            Op::Load { addr: a } => *a = slot,
            Op::Store { addr: a, .. } => *a = slot,
            _ => {}
        }
    }

    // region exits into L: a block that reloads
    let n0 = f.blocks.len();
    let mut reload_blocks: Vec<usize> = Vec::new();
    for (_, body) in &plan.regions {
        for b in 0..n0 {
            if !body[b] {
                continue;
            }
            let succs = f.blocks[b].term.successors();
            let mut seen: Vec<usize> = Vec::new();
            for s in succs {
                let s = s as usize;
                if seen.contains(&s) {
                    continue;
                }
                seen.push(s);
                if l.has(s) && !body[s] {
                    let z = split_edge(f, b, s);
                    let v = f.push(z as BlockId, ty, Op::Load { addr });
                    f.push_void(z as BlockId, ty, Op::Store { addr: slot, val: v });
                    reload_blocks.push(z);
                }
            }
        }
    }
    let n = f.blocks.len();
    let mut live = plan.live.clone();
    live.resize(n, false);
    for &z in &reload_blocks {
        live[z] = true;
    }
    let region_ph: HashSet<usize> = plan.regions.iter().map(|(p, _)| *p).collect();

    // dirty data flow over the live blocks
    let preds = crate::mem2reg::preds(f);
    let x = l.head;
    let is_store_to_slot = |i: &Inst| matches!(&i.op, Op::Store { addr: a, .. } if *a == slot);
    // transfer: in-state -> out-state (true = dirty)
    let transfer = |f: &Func, b: usize, mut d: bool| -> bool {
        if reload_blocks.contains(&b) {
            d = false;
        }
        for (ii, i) in f.blocks[b].insts.iter().enumerate() {
            if is_store_to_slot(i) && !reload_blocks.contains(&b) {
                d = true;
            } else if d && plan.reads.contains(&(b, ii)) {
                d = false;
            }
        }
        if region_ph.contains(&b) {
            d = false;
        }
        d
    };
    let mut din = vec![false; n];
    let mut dout = vec![false; n];
    for _ in 0..(n + 4) {
        let mut changed = false;
        for b in 0..n {
            if !live[b] {
                continue;
            }
            let mut d = false;
            if !reload_blocks.contains(&b) {
                for &p in &preds[b] {
                    if p == ph && b == x {
                        continue; // clean after the preheader load
                    }
                    if p < n && live[p] {
                        d |= dout[p];
                    }
                }
            }
            let o = transfer(f, b, d);
            if d != din[b] || o != dout[b] {
                din[b] = d;
                dout[b] = o;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // write-backs: in front of reads while dirty, at the end of region
    // preheaders while dirty
    for b in 0..n {
        if !live[b] || reload_blocks.contains(&b) {
            continue;
        }
        let mut d = din[b];
        let old = std::mem::take(&mut f.blocks[b].insts);
        let mut new: Vec<Inst> = Vec::with_capacity(old.len() + 2);
        for (ii, i) in old.into_iter().enumerate() {
            if is_store_to_slot(&i) {
                new.push(i);
                d = true;
                continue;
            }
            if d && plan.reads.contains(&(b, ii)) {
                let v = f.new_val_pub(ty);
                new.push(Inst::like(Some(v), ty, Op::Load { addr: slot }, i.loc));
                new.push(Inst::like(None, ty, Op::Store { addr, val: v }, i.loc));
                d = false;
            }
            new.push(i);
        }
        if region_ph.contains(&b) && d {
            let v = f.new_val_pub(ty);
            new.push(Inst::new(Some(v), ty, Op::Load { addr: slot }));
            new.push(Inst::new(None, ty, Op::Store { addr, val: v }));
        }
        f.blocks[b].insts = new;
    }
    // leaving L from a dirty point: write back on the edge
    for b in 0..n {
        if !live[b] || !dout[b] {
            continue;
        }
        let succs = f.blocks[b].term.successors();
        let mut seen: Vec<usize> = Vec::new();
        for s in succs {
            let s = s as usize;
            if seen.contains(&s) {
                continue;
            }
            seen.push(s);
            let inside = s < l.body.len() && l.body[s] || reload_blocks.contains(&s);
            if !inside {
                let z = split_edge(f, b, s);
                let v = f.push(z as BlockId, ty, Op::Load { addr: slot });
                f.push_void(z as BlockId, ty, Op::Store { addr, val: v });
            }
        }
    }
    f.loc_stamp = saved_stamp;
}
