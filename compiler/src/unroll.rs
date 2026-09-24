// SPDX-License-Identifier: MPL-2.0
//! **Round TEMPO 14 -- full unrolling of short loops with a known trip count.**
//!
//! ## Why this pass exists
//!
//! The hot loop of the MP3 decoder (`synth` in `lib/ton/mp3.fi`) is
//!
//! ```firn
//! var k: i32 = 0
//! while k < 8 {
//!     ...
//!     if k == 0 { ... } else { if (k & 1) == 1 { ... } else { ... } }
//!     k = k + 1
//! }
//! ```
//!
//! Eight passes, and every one of them pays for the loop test, for
//! `k == 0` and for `k & 1` -- three compares and three branches that decide
//! nothing the compiler could not have decided itself: the trip count and
//! every value of `k` are known at compile time. `gcc -O2` peels such a loop
//! completely ("complete unrolling"), and then the branches on `k` fold
//! away. Measured before this round: `synth` 35.0 million instructions in
//! Firn, 22.7 million in C (callgrind, 8 s of sound).
//!
//! ## The shape it recognises
//!
//! After `mem2reg` and `licm` a counted loop looks like this in FIR:
//!
//! ```text
//! P:    ...                          <- the one predecessor outside the loop
//!       br H
//! H:    %i = phi [P %c0, L %i2]      <- the counter, plus any other phis
//!       ...
//!       %c = cmp.lt.i32 %i, %limit   <- %limit a constant
//!       brcond %c, B, X
//! B..L: the body (any number of blocks, branches and phis inside)
//!       %i2 = add %i, %step          <- %step a constant
//!       br H                         <- the one back edge
//! ```
//!
//! The trip count is found by SIMULATING the counter in its own type (with
//! its wrapping and its signedness) -- not by a formula, so that `<=`, `!=`,
//! counting down and odd steps are all the same question. A checked
//! increment (`dev-fast`, `release-safe`) is accepted only when the
//! simulation shows it cannot overflow on any pass the loop really makes.
//!
//! ## The transformation
//!
//! The loop blocks are copied once per pass. In copy `j` every header phi is
//! not an instruction any more but a NAME for a value that already exists:
//! the preheader's value for `j == 0`, otherwise the back edge value of copy
//! `j - 1`. The header's `brcond` becomes a plain `br` into the body; the
//! back edge of copy `j` jumps into the header of copy `j + 1`. One more
//! header copy after the last pass evaluates the header once more (its
//! instructions may be used behind the loop) and jumps to the exit.
//!
//! Nothing is folded here. The counter in copy `j` is `add` of constants,
//! `k == 0` is `cmp` of constants -- `fold`, `simplify-term`,
//! `merge-blocks` and `dce` do the rest in the same fixpoint loop. The
//! original loop becomes unreachable and `dce` removes it.
//!
//! ## The conditions, and why each one is needed
//!
//! * **Exactly one back edge, exactly one predecessor outside, and the
//!   header is the ONLY block that leaves the loop.** A `break` would make
//!   the exit block a join of many copies; that is possible but it is a
//!   different pass. A `return` inside the body leaves the loop too, so
//!   such a loop stays as it is.
//! * **Innermost loops only.** A loop inside the body would be copied as a
//!   whole with its back edge; once the inner one is unrolled the outer one
//!   is looked at again in the next round anyway.
//! * **No inline assembler, no `clone`.** An `asm` block may carry local
//!   labels, and copying it would define them twice.
//! * **A size limit.** At most `MAX_TRIPS` passes and at most `BUDGET`
//!   instructions after copying (`FIRN_UNROLL_BUDGET` overrides it for
//!   measurements). Larger loops stay loops: past a point the code grows
//!   faster than the branches it saves, and the instruction cache pays.
//! * **Nothing secret, no `#[constant_time]`.** Not because the copy would
//!   be wrong, but because those functions promise a shape and this pass
//!   has no business changing it.
//!
//! `FIRN_NO_UNROLL=1` switches the pass off for comparison measurements.

use crate::fir::{BlockId, CmpOp, FTy, Func, Inst, Op, Term, Val};
use std::collections::HashMap;

/// At most this many passes are unrolled.
const MAX_TRIPS: u64 = 16;
/// At most this many instructions after unrolling (phis not counted).
const BUDGET: usize = 640;

fn budget() -> usize {
    std::env::var("FIRN_UNROLL_BUDGET")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(BUDGET)
}

/// Run the pass; returns the number of unrolled loops.
pub fn run(f: &mut Func) -> usize {
    if std::env::var_os("FIRN_NO_UNROLL").is_some() {
        return 0;
    }
    if f.constant_time || !f.secret.is_empty() || !f.has_phi() {
        return 0;
    }
    let mut n = 0;
    // One loop per round: the analysis is rebuilt after every change, so
    // no stale block or value numbers are ever used.
    while n < 64 {
        match find(f) {
            Some(plan) => {
                apply(f, &plan);
                n += 1;
            }
            None => break,
        }
    }
    n
}

struct Plan {
    /// preheader, header, exit
    p: usize,
    h: usize,
    x: usize,
    /// the successor of the header inside the loop
    body: usize,
    /// the one block with the back edge
    latch: usize,
    /// all loop blocks, header first, in block order after that
    blocks: Vec<usize>,
    trips: u64,
}

fn preds_of(f: &Func) -> Vec<Vec<usize>> {
    let nb = f.blocks.len();
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); nb];
    for (i, b) in f.blocks.iter().enumerate() {
        for s in b.term.successors() {
            let s = s as usize;
            if s < nb && !preds[s].contains(&i) {
                preds[s].push(i);
            }
        }
    }
    preds
}

fn find(f: &Func) -> Option<Plan> {
    let nb = f.blocks.len();
    if nb < 2 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return None;
    }
    let preds = preds_of(f);
    let dom = crate::mem2reg::dominators(f);
    // Reachable blocks only (an unreachable block "dominates" nonsense).
    let mut reach = vec![false; nb];
    let mut stack = vec![0usize];
    reach[0] = true;
    while let Some(b) = stack.pop() {
        for s in f.blocks[b].term.successors() {
            let s = s as usize;
            if s < nb && !reach[s] {
                reach[s] = true;
                stack.push(s);
            }
        }
    }
    // Where is each value defined, and which values are constants?
    let nv = f.val_types.len();
    let mut defblock: Vec<usize> = vec![usize::MAX; nv];
    let mut consts: Vec<Option<i128>> = vec![None; nv];
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            if let Some(d) = i.dst {
                let d = d as usize;
                if d < nv {
                    defblock[d] = bi;
                    if let Op::Const(c) = i.op {
                        consts[d] = Some(c);
                    }
                }
            }
        }
    }
    let limit = budget();

    'head: for h in 0..nb {
        if !reach[h] || !f.blocks[h].has_phi() {
            continue;
        }
        // back edges into h
        let latches: Vec<usize> =
            preds[h].iter().copied().filter(|&p| reach[p] && dom[p][h]).collect();
        if latches.len() != 1 {
            continue;
        }
        let latch = latches[0];
        let outside: Vec<usize> = preds[h].iter().copied().filter(|&p| p != latch).collect();
        if outside.len() != 1 {
            continue;
        }
        let p = outside[0];
        // the loop: h plus everything that reaches the latch without h
        let mut inl = vec![false; nb];
        inl[h] = true;
        let mut work = vec![latch];
        while let Some(b) = work.pop() {
            if inl[b] {
                continue;
            }
            inl[b] = true;
            for &q in &preds[b] {
                if !inl[q] {
                    work.push(q);
                }
            }
        }
        let blocks: Vec<usize> = std::iter::once(h)
            .chain((0..nb).filter(|&b| inl[b] && b != h))
            .collect();
        // every loop block dominated by h, reachable, no second back edge,
        // no exit except through h
        for &b in &blocks {
            if !reach[b] || !dom[b][h] {
                continue 'head;
            }
            for s in f.blocks[b].term.successors() {
                let s = s as usize;
                if s >= nb {
                    continue 'head;
                }
                if inl[s] {
                    // an edge back to a block that dominates b is a loop
                    if dom[b][s] && !(s == h && b == latch) {
                        continue 'head;
                    }
                } else if b != h {
                    continue 'head;
                }
            }
        }
        // the header: brcond on a compare of a header phi with a constant
        let (cond, then_bb, else_bb) = match f.blocks[h].term {
            Term::BrCond { cond, then_bb, else_bb } => (cond, then_bb as usize, else_bb as usize),
            _ => continue,
        };
        let (body, x, stay_if) = match (inl[then_bb], inl[else_bb]) {
            (true, false) => (then_bb, else_bb, true),
            (false, true) => (else_bb, then_bb, false),
            _ => continue,
        };
        if x == h || body == x {
            continue;
        }
        let cmp = match f.blocks[h].insts.iter().find(|i| i.dst == Some(cond)) {
            Some(i) => i,
            None => continue,
        };
        let (op, cty, a, b) = match cmp.op {
            Op::Cmp { op, ty, a, b } => (op, ty, a, b),
            _ => continue,
        };
        if cty.is_float() || cty == FTy::Ptr || cty == FTy::V128 || cty == FTy::Bool {
            continue;
        }
        // which side is the counter?
        let (iv, lim, op) = match (consts.get(b as usize).copied().flatten(), consts.get(a as usize).copied().flatten()) {
            (Some(l), _) => (a, l, op),
            (None, Some(l)) => (b, l, swap(op)),
            _ => continue,
        };
        let phi = match f.blocks[h].insts.iter().find(|i| i.dst == Some(iv)) {
            Some(i) => i,
            None => continue,
        };
        let incoming = match &phi.op {
            Op::Phi { incoming } => incoming,
            _ => continue,
        };
        let init = match incoming.iter().find(|(q, _)| *q as usize == p) {
            Some((_, v)) => *v,
            None => continue,
        };
        let next = match incoming.iter().find(|(q, _)| *q as usize == latch) {
            Some((_, v)) => *v,
            None => continue,
        };
        let init = match consts.get(init as usize).copied().flatten() {
            Some(c) => c,
            None => continue,
        };
        // the step: next = iv (+|-) const, defined inside the loop
        let nd = next as usize;
        if nd >= nv || defblock[nd] == usize::MAX || !inl[defblock[nd]] {
            continue;
        }
        let ndef = match f.blocks[defblock[nd]].insts.iter().find(|i| i.dst == Some(next)) {
            Some(i) => i,
            None => continue,
        };
        let (bop, x1, x2, checked) = match &ndef.op {
            Op::Bin(o, x1, x2) => (*o, *x1, *x2, false),
            Op::BinWrapSat { kind: crate::fir::WrapSatKind::Wrap, op, a, b } => (*op, *a, *b, false),
            Op::CheckedBin { op, a, b, .. } => (*op, *a, *b, true),
            _ => continue,
        };
        let step = match bop {
            crate::fir::BinOp::Add => {
                if x1 == iv {
                    consts.get(x2 as usize).copied().flatten()
                } else if x2 == iv {
                    consts.get(x1 as usize).copied().flatten()
                } else {
                    None
                }
            }
            crate::fir::BinOp::Sub if x1 == iv => consts.get(x2 as usize).copied().flatten().map(|c| -c),
            _ => None,
        };
        let step = match step {
            Some(s) => s,
            None => continue,
        };
        let ty = ndef.ty;
        if ty != cty {
            continue;
        }
        // simulate
        let mut v = ty.truncate(init);
        let mut trips: u64 = 0;
        loop {
            let c = compare(op, cty, v, ty.truncate(lim));
            if c != stay_if {
                break;
            }
            trips += 1;
            if trips > MAX_TRIPS {
                continue 'head;
            }
            let raw = v + step;
            let w = ty.truncate(raw);
            if checked && w != raw {
                // the checked increment would fire: leave the loop alone
                continue 'head;
            }
            v = w;
        }
        if trips == 0 {
            continue;
        }
        // size and forbidden instructions
        let mut size = 0usize;
        for &bb in &blocks {
            for i in &f.blocks[bb].insts {
                match i.op {
                    Op::Phi { .. } => {}
                    Op::Asm { .. } | Op::ThreadSpawn { .. } | Op::Alloca { .. } => continue 'head,
                    _ => size += 1,
                }
            }
            size += 1; // the terminator
        }
        let fits = size.saturating_mul(trips as usize) <= limit;
        if std::env::var_os("FIRN_UNROLL_TRACE").is_some() {
            eprintln!("unroll {} bb{}: {} passes x {} = {} {}", f.name, h, trips, size,
                      size * trips as usize, if fits { "UNROLLED" } else { "too big" });
        }
        if !fits {
            continue;
        }
        // a value of a body block (not the header) must not be read outside
        for bb in 0..nb {
            if inl[bb] {
                continue;
            }
            let mut uses = Vec::new();
            for i in &f.blocks[bb].insts {
                i.op.uses(&mut uses);
            }
            term_uses(&f.blocks[bb].term, &mut uses);
            for u in uses {
                let d = defblock.get(u as usize).copied().unwrap_or(usize::MAX);
                if d != usize::MAX && inl[d] && d != h {
                    continue 'head;
                }
            }
        }
        return Some(Plan { p, h, x, body, latch, blocks, trips });
    }
    None
}

fn swap(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Lt => CmpOp::Gt,
        CmpOp::Le => CmpOp::Ge,
        CmpOp::Gt => CmpOp::Lt,
        CmpOp::Ge => CmpOp::Le,
        o => o,
    }
}

/// The compare as the machine does it: values are already truncated to
/// their type (sign extended for signed types), so an unsigned compare has
/// to look at the bit pattern.
fn compare(op: CmpOp, ty: FTy, a: i128, b: i128) -> bool {
    let (a, b) = if ty.signed() {
        (a, b)
    } else {
        let m: i128 = if ty.bits() >= 64 { (1i128 << 64) - 1 } else { (1i128 << ty.bits()) - 1 };
        (a & m, b & m)
    };
    match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::Lt => a < b,
        CmpOp::Le => a <= b,
        CmpOp::Gt => a > b,
        CmpOp::Ge => a >= b,
    }
}

fn term_uses(t: &Term, out: &mut Vec<Val>) {
    match t {
        Term::BrCond { cond, .. } => out.push(*cond),
        Term::Switch { val, .. } => out.push(*val),
        Term::Ret(Some(v)) => out.push(*v),
        _ => {}
    }
}

/// EVERY operand, including those of the instructions that
/// `Op::for_each_use_mut` leaves alone on purpose (select, barrier,
/// secure_zero, MMIO): a copy has to read the copied values, not the
/// original ones -- that is not a rewrite of the instruction, it is a new
/// instance of it.
fn map_all(op: &mut Op, m: &HashMap<Val, Val>) {
    let r = |v: &mut Val| {
        if let Some(&n) = m.get(v) {
            *v = n;
        }
    };
    match op {
        Op::Select { cond, a, b } => {
            r(cond);
            r(a);
            r(b);
        }
        Op::Barrier { val } => r(val),
        Op::SecureZero { addr, size } => {
            r(addr);
            r(size);
        }
        Op::MmioLoad { addr } => r(addr),
        Op::MmioStore { addr, val } => {
            r(addr);
            r(val);
        }
        Op::Asm { ins, outs, .. } => {
            for v in ins.iter_mut() {
                r(v);
            }
            for v in outs.iter_mut() {
                r(v);
            }
        }
        other => other.for_each_use_mut(|v| r(v)),
    }
}

fn apply(f: &mut Func, pl: &Plan) {
    let h = pl.h;
    let trips = pl.trips as usize;
    // header phis: (dst, value from the preheader, value from the latch)
    let mut hphis: Vec<(Val, Val, Val)> = Vec::new();
    for i in &f.blocks[h].insts {
        if let Op::Phi { incoming } = &i.op {
            let d = i.dst.expect("phi without a result");
            let from_p = incoming.iter().find(|(q, _)| *q as usize == pl.p).map(|(_, v)| *v).unwrap();
            let from_l = incoming.iter().find(|(q, _)| *q as usize == pl.latch).map(|(_, v)| *v).unwrap();
            hphis.push((d, from_p, from_l));
        }
    }
    // block numbers of all copies: copy j < trips gets every loop block,
    // copy `trips` only the header
    let mut bmap: Vec<HashMap<usize, BlockId>> = Vec::new();
    for j in 0..=trips {
        let mut m = HashMap::new();
        for &b in &pl.blocks {
            if j == trips && b != h {
                continue;
            }
            m.insert(b, f.add_block());
        }
        bmap.push(m);
    }
    let resolve = |m: &HashMap<Val, Val>, v: Val| -> Val { m.get(&v).copied().unwrap_or(v) };
    let mut prev: HashMap<Val, Val> = HashMap::new();
    for j in 0..=trips {
        let mut vm: HashMap<Val, Val> = HashMap::new();
        for &(d, from_p, from_l) in &hphis {
            let v = if j == 0 { from_p } else { resolve(&prev, from_l) };
            vm.insert(d, v);
        }
        // fresh values for everything else defined in this copy
        for &b in &pl.blocks {
            if j == trips && b != h {
                continue;
            }
            let skip = if b == h { f.blocks[b].phi_count() } else { 0 };
            let dsts: Vec<(Val, FTy)> = f.blocks[b].insts[skip..]
                .iter()
                .filter_map(|i| i.dst.map(|d| (d, f.val_ty(d))))
                .collect();
            for (d, t) in dsts {
                let nvv = f.new_val_pub(t);
                if f.no_coalesce.contains(&d) {
                    f.no_coalesce.insert(nvv);
                }
                vm.insert(d, nvv);
            }
        }
        // clone
        for &b in &pl.blocks {
            if j == trips && b != h {
                continue;
            }
            let nbid = bmap[j][&b];
            let skip = if b == h { f.blocks[b].phi_count() } else { 0 };
            let mut insts: Vec<Inst> = Vec::with_capacity(f.blocks[b].insts.len());
            for i in &f.blocks[b].insts[skip..] {
                let mut ni = i.clone();
                if let Some(d) = ni.dst {
                    ni.dst = Some(vm[&d]);
                }
                if let Op::Phi { incoming } = &mut ni.op {
                    for (q, v) in incoming.iter_mut() {
                        *q = bmap[j][&(*q as usize)];
                        *v = resolve(&vm, *v);
                    }
                    incoming.sort_by_key(|(q, _)| *q);
                } else {
                    map_all(&mut ni.op, &vm);
                }
                insts.push(ni);
            }
            let tb = |s: BlockId| -> BlockId {
                let s = s as usize;
                if s == h {
                    // the back edge: into the header of the next copy
                    bmap[j + 1][&h]
                } else if let Some(&n) = bmap[j].get(&s) {
                    n
                } else {
                    s as BlockId
                }
            };
            let term = if b == h {
                if j == trips {
                    Term::Br(pl.x as BlockId)
                } else {
                    Term::Br(bmap[j][&pl.body])
                }
            } else {
                match &f.blocks[b].term {
                    Term::Br(s) => Term::Br(tb(*s)),
                    Term::BrCond { cond, then_bb, else_bb } => Term::BrCond {
                        cond: resolve(&vm, *cond),
                        then_bb: tb(*then_bb),
                        else_bb: tb(*else_bb),
                    },
                    Term::Switch { val, ty, cases, default } => Term::Switch {
                        val: resolve(&vm, *val),
                        ty: *ty,
                        cases: cases.iter().map(|(c, s)| (*c, tb(*s))).collect(),
                        default: tb(*default),
                    },
                    Term::Ret(v) => Term::Ret(v.map(|v| resolve(&vm, v))),
                    Term::Unset => Term::Unset,
                }
            };
            let blk = &mut f.blocks[nbid as usize];
            blk.insts = insts;
            blk.term = term;
        }
        prev = vm;
    }
    let last_h = bmap[trips][&h];
    // the preheader now enters copy 0
    let first_h = bmap[0][&h];
    retarget(&mut f.blocks[pl.p].term, h as BlockId, first_h);
    // behind the loop: header values are those of the last header copy,
    // and the exit's phis come from it
    let header_vals: Vec<Val> = f.blocks[h].insts.iter().filter_map(|i| i.dst).collect();
    let mut outm: HashMap<Val, Val> = HashMap::new();
    for d in header_vals {
        outm.insert(d, resolve(&prev, d));
    }
    let loopset: Vec<usize> = pl.blocks.clone();
    let nb = f.blocks.len();
    let first_new = nb - bmap.iter().map(|m| m.len()).sum::<usize>();
    for bi in 0..nb {
        if loopset.contains(&bi) || bi >= first_new {
            continue;
        }
        let blk = &mut f.blocks[bi];
        for i in blk.insts.iter_mut() {
            if let Op::Phi { incoming } = &mut i.op {
                if bi == pl.x {
                    for (q, _) in incoming.iter_mut() {
                        if *q as usize == h {
                            *q = last_h;
                        }
                    }
                    incoming.sort_by_key(|(q, _)| *q);
                }
                for (_, v) in incoming.iter_mut() {
                    if let Some(&n) = outm.get(v) {
                        *v = n;
                    }
                }
            } else {
                map_all(&mut i.op, &outm);
            }
        }
        match &mut blk.term {
            Term::BrCond { cond, .. } => {
                if let Some(&n) = outm.get(cond) {
                    *cond = n;
                }
            }
            Term::Switch { val, .. } => {
                if let Some(&n) = outm.get(val) {
                    *val = n;
                }
            }
            Term::Ret(Some(v)) => {
                if let Some(&n) = outm.get(v) {
                    *v = n;
                }
            }
            _ => {}
        }
    }
    // The old loop is unreachable now. Empty it right away, so that the phi
    // invariants hold at once (its header still has an entry for the
    // preheader, and it is an edge into the exit): every block becomes a
    // branch to itself -- no edge out, no return of the wrong type in a
    // function that returns a value. `dce` removes them.
    for &b in &pl.blocks {
        f.blocks[b].insts.clear();
        f.blocks[b].term = Term::Br(b as BlockId);
    }
}

fn retarget(t: &mut Term, from: BlockId, to: BlockId) {
    match t {
        Term::Br(s) => {
            if *s == from {
                *s = to;
            }
        }
        Term::BrCond { then_bb, else_bb, .. } => {
            if *then_bb == from {
                *then_bb = to;
            }
            if *else_bb == from {
                *else_bb = to;
            }
        }
        Term::Switch { cases, default, .. } => {
            for (_, s) in cases.iter_mut() {
                if *s == from {
                    *s = to;
                }
            }
            if *default == from {
                *default = to;
            }
        }
        _ => {}
    }
}
