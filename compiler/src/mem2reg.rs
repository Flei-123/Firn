//! Memory -> value: resolving `alloca`/`store`/`load`, copy propagation
//! and block merging.
//!
//! This file holds the passes that were missing at round 1 and got rightly
//! criticised:
//!
//!  * **mem2reg (alloca written once):** an `alloca` whose pointer escapes
//!    nowhere (used only as the address of a `load`/`store`) and that is
//!    written EXACTLY ONCE, where the `store` **dominates** every `load`, is
//!    resolved: every `load` is replaced by the stored value. FIR knows no
//!    phi nodes — that is why the dominance condition is mandatory here and
//!    not merely an optimization (cells written several times stay in
//!    memory; the register allocator (`regalloc.rs`) keeps those in a
//!    register permanently instead).
//!  * **local store forwarding:** `store p, v` followed by `load p` in
//!    the same block without a memory effect between them -> the `load`
//!    becomes `v`. Likewise `load p` ... `load p` (common subexpression).
//!  * **copy propagation / algebraic simplification:** identity `cast`,
//!    `x+0`, `x-0`, `x*1`, `x*0`, `x|0`, `x^0`, `x&-1`, `x<<0`, `x>>0`,
//!    `x/1`, `ptradd p, 0`.
//!  * **block merging:** `A: ... br B` with B as the only successor and A as
//!    the only predecessor -> B is appended to A. Empty blocks holding a
//!    pure `br C` are bridged (jump threading).
//!
//! HARD RULE (SPEC §9.2): `Op::Select`, `Op::Barrier`, `Op::SecureZero` and
//! every value from `f.secret` are NEVER changed, replaced or removed
//! here. Their operands are not rewritten; a `select` never turns into a
//! branch.

use crate::fir::{BinOp, Func, Inst, Op, Term, Val};
use std::collections::HashMap;

/// Instructions that the optimizer treats as untouchable (SPEC §9 and
/// — since round 52 — SPEC §2: inline assembler and MMIO are `volatile`).
pub(crate) fn is_untouchable(op: &Op) -> bool {
    matches!(
        op,
        Op::Select { .. }
            | Op::Barrier { .. }
            | Op::SecureZero { .. }
            | Op::Asm { .. }
            | Op::MmioLoad { .. }
            | Op::MmioStore { .. }
    )
}

// ----------------------------------------------------------------- Helpers ---

/// Predecessor lists. Presumes the FIR invariant `blocks[i].id == i`.
pub(crate) fn preds(f: &Func) -> Vec<Vec<usize>> {
    let n = f.blocks.len();
    let mut p = vec![Vec::new(); n];
    for (i, b) in f.blocks.iter().enumerate() {
        for s in b.term.successors() {
            let s = s as usize;
            if s < n && !p[s].contains(&i) {
                p[s].push(i);
            }
        }
    }
    p
}

/// `dom[b][d] == true`  <=>  block `d` dominates block `b`.
///
/// ROUND 87 -- THE SAME SET, IN WORDS INSTEAD OF OCTETS.
///
/// The data flow underneath is unchanged (dom(b) = {b} + the intersection of
/// dom over all predecessors, iterated to the fixed point), and so is the
/// result. What changed is what one round costs: the sets used to be
/// `Vec<bool>` -- one OCTET per block -- and every block allocated two fresh
/// ones per round. A function with 500 blocks moved a quarter of a megabyte
/// per round through the cache and allocated a thousand vectors.
///
/// Now the sets are words: 64 blocks per `u64`, the intersection is an `&`
/// over `n/64` words, and nothing is allocated inside the loop. For 500
/// blocks that is eight words instead of 500 octets per intersection, and
/// the allocations are gone entirely.
///
/// The `Vec<Vec<bool>>` at the end stays, because that is what the callers
/// read; building it costs one pass over the result, which is the size of
/// the result anyway.
pub(crate) fn dominators(f: &Func) -> Vec<Vec<bool>> {
    let n = f.blocks.len();
    if n == 0 {
        return Vec::new();
    }
    let pr = preds(f);
    let w = n.div_ceil(64);
    // dom[b] as words. Start: block 0 is dominated by itself alone,
    // everything else provisionally by everybody.
    let mut dom = vec![0u64; n * w];
    let full_last = if n % 64 == 0 { !0u64 } else { (1u64 << (n % 64)) - 1 };
    for b in 1..n {
        for k in 0..w {
            dom[b * w + k] = if k + 1 == w { full_last } else { !0u64 };
        }
    }
    dom[0] = 1; // block 0: only itself
    let mut new = vec![0u64; w];
    let mut rounds = 0;
    loop {
        rounds += 1;
        let mut changed = false;
        for b in 1..n {
            if pr[b].is_empty() {
                // unreachable: dominated by nothing but itself
                for k in 0..w {
                    new[k] = 0;
                }
            } else {
                let first = pr[b][0] * w;
                new[..w].copy_from_slice(&dom[first..first + w]);
                for &p in &pr[b][1..] {
                    let base = p * w;
                    for k in 0..w {
                        new[k] &= dom[base + k];
                    }
                }
            }
            new[b >> 6] |= 1u64 << (b & 63);
            if new[..w] != dom[b * w..b * w + w] {
                dom[b * w..b * w + w].copy_from_slice(&new[..w]);
                changed = true;
            }
        }
        if !changed || rounds > n + 2 {
            break;
        }
    }
    let mut out = vec![vec![false; n]; n];
    for b in 0..n {
        for d in 0..n {
            out[b][d] = dom[b * w + (d >> 6)] & (1u64 << (d & 63)) != 0;
        }
    }
    out
}

/// Is the value `v` untouchable (secret) in `f`?
fn locked(f: &Func, v: Val) -> bool {
    f.is_secret(v)
}

/// Replaces uses per `map` (plain substitution only, no chains).
/// Yields the number of rewritten operands.
pub(crate) fn replace_uses(f: &mut Func, map: &HashMap<Val, Val>) -> usize {
    if map.is_empty() {
        return 0;
    }
    let secret: Vec<Val> = f.secret.iter().copied().collect();
    let is_locked = |v: Val| secret.contains(&v);
    let mut n = 0usize;
    let rep = |v: &mut Val, n: &mut usize| {
        if let Some(&nv) = map.get(v) {
            if !is_locked(*v) && !is_locked(nv) {
                *v = nv;
                *n += 1;
            }
        }
    };
    for b in f.blocks.iter_mut() {
        for i in b.insts.iter_mut() {
            if is_untouchable(&i.op) {
                continue; // SPEC §9.2: operands stay as they are
            }
            match &mut i.op {
                Op::Const(_) | Op::Alloca { .. } | Op::GcAddr { .. } | Op::ThreadSelf => {}
                Op::Bin(_, a, b2) => {
                    rep(a, &mut n);
                    rep(b2, &mut n);
                }
                Op::Cmp { a, b: b2, .. } => {
                    rep(a, &mut n);
                    rep(b2, &mut n);
                }
                Op::Un(_, a) => rep(a, &mut n),
                Op::Cast { src, .. } => rep(src, &mut n),
                Op::Load { addr } => rep(addr, &mut n),
                Op::Store { addr, val } => {
                    rep(addr, &mut n);
                    rep(val, &mut n);
                }
                Op::PtrAdd { base, off } => {
                    rep(base, &mut n);
                    rep(off, &mut n);
                }
                Op::Simd { args, .. } => {
                    for a in args.iter_mut() {
                        rep(a, &mut n);
                    }
                }
                Op::Call { args, .. } | Op::Syscall { args } => {
                    for a in args.iter_mut() {
                        rep(a, &mut n);
                    }
                }
                Op::CallIndirect { target, args } => {
                    rep(target, &mut n);
                    for a in args.iter_mut() {
                        rep(a, &mut n);
                    }
                }
                Op::VtabAddr { .. } | Op::FnRef { .. } => {}
                Op::CopyMem { dst, src, .. } => {
                    rep(dst, &mut n);
                    rep(src, &mut n);
                }
                Op::AtomicCas { addr, erw, new } => {
                    rep(addr, &mut n);
                    rep(erw, &mut n);
                    rep(new, &mut n);
                }
                Op::ThreadSpawn { arg, stack, ctid } => {
                    rep(arg, &mut n);
                    rep(stack, &mut n);
                    rep(ctid, &mut n);
                }
                Op::AtomicAdd { addr, val } => {
                    rep(addr, &mut n);
                    rep(val, &mut n);
                }
                Op::Select { .. } | Op::Barrier { .. } | Op::SecureZero { .. } => {}
                // ROUND 52: volatile — the operands are NOT
                // rewritten (like select/barrier/secure_zero).
                Op::Asm { .. } | Op::MmioLoad { .. } | Op::MmioStore { .. } => {}
            }
        }
        match &mut b.term {
            Term::BrCond { cond, .. } => rep(cond, &mut n),
            Term::Switch { val, .. } => rep(val, &mut n),
            Term::Ret(Some(v)) => rep(v, &mut n),
            _ => {}
        }
    }
    n
}

// ------------------------------------------------------------------ mem2reg ---

/// Describes how an `alloca` is used.
struct CellUse {
    /// only as the address of a load/store (no ptradd, no call argument, ...)
    simple: bool,
    stores: Vec<(usize, usize)>, // (block, index)
    loads: Vec<(usize, usize)>,
}

fn scan_cells(f: &Func) -> HashMap<Val, CellUse> {
    let mut cells: HashMap<Val, CellUse> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { .. }) = (i.dst, &i.op) {
                cells.insert(d, CellUse { simple: true, stores: Vec::new(), loads: Vec::new() });
            }
        }
    }
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            match &i.op {
                Op::Load { addr } => {
                    if let Some(c) = cells.get_mut(addr) {
                        c.loads.push((bi, ii));
                    }
                }
                Op::Store { addr, val } => {
                    if let Some(c) = cells.get_mut(addr) {
                        c.stores.push((bi, ii));
                    }
                    if let Some(c) = cells.get_mut(val) {
                        c.simple = false; // pointer escapes as a value
                    }
                }
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    for v in buf.iter() {
                        if let Some(c) = cells.get_mut(v) {
                            c.simple = false;
                        }
                    }
                }
            }
        }
        match &b.term {
            Term::Ret(Some(v)) => {
                if let Some(c) = cells.get_mut(v) {
                    c.simple = false;
                }
            }
            Term::BrCond { cond, .. } => {
                if let Some(c) = cells.get_mut(cond) {
                    c.simple = false;
                }
            }
            Term::Switch { val, .. } => {
                if let Some(c) = cells.get_mut(val) {
                    c.simple = false;
                }
            }
            _ => {}
        }
    }
    cells
}

/// Resolves `alloca`s written exactly once whose `store` dominates every
/// `load`. Yields the number of replaced `load`s.
pub(crate) fn promote_single_store(f: &mut Func) -> usize {
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let cells = scan_cells(f);
    // ROUND 82: no cell, nothing to promote — and above all no dominator
    // matrix to build. A function without an `alloca` paid for it before.
    if cells.is_empty() {
        return 0;
    }
    let dom = dominators(f);
    let mut map: HashMap<Val, Val> = HashMap::new();
    for (cell, u) in cells.iter() {
        if !u.simple || u.stores.len() != 1 || u.loads.is_empty() || locked(f, *cell) {
            continue;
        }
        let (sb, si) = u.stores[0];
        let (sty, sval) = match &f.blocks[sb].insts[si].op {
            Op::Store { val, .. } => (f.blocks[sb].insts[si].ty, *val),
            _ => continue,
        };
        if locked(f, sval) {
            continue;
        }
        // The stored value must not be the cell itself.
        if sval == *cell {
            continue;
        }
        let mut ok = true;
        for &(lb, li) in &u.loads {
            let lty = f.blocks[lb].insts[li].ty;
            if lty != sty {
                ok = false; // other width: memory semantics, do not touch
                break;
            }
            let dominates = if lb == sb { li > si } else { dom[lb][sb] };
            if !dominates {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        for &(lb, li) in &u.loads {
            if let Some(d) = f.blocks[lb].insts[li].dst {
                if !locked(f, d) {
                    map.insert(d, sval);
                }
            }
        }
    }
    if map.is_empty() {
        return 0;
    }
    let n = map.len();
    replace_uses(f, &map);
    n
}

/// Removes `alloca`s whose pointer does not escape and that are NEVER read:
/// together with every `store` into them (dead store). Exactly that is left
/// over after `promote_single_store` has resolved the `load`s — in round 1
/// this remainder stayed. Yields the number of removed instructions.
pub(crate) fn remove_dead_stores(f: &mut Func) -> usize {
    let cells = scan_cells(f);
    let dead: Vec<Val> = cells
        .iter()
        .filter(|(v, u)| u.simple && u.loads.is_empty() && !locked(f, **v))
        .map(|(v, _)| *v)
        .collect();
    if dead.is_empty() {
        return 0;
    }
    let mut n = 0usize;
    for b in f.blocks.iter_mut() {
        let before = b.insts.len();
        b.insts.retain(|i| match &i.op {
            Op::Store { addr, .. } => !dead.contains(addr),
            Op::Alloca { .. } => match i.dst {
                Some(d) => !dead.contains(&d),
                None => true,
            },
            _ => true,
        });
        n += before - b.insts.len();
    }
    n
}

// ----------------------------------------------------- local store forwarding ---

/// Does this instruction change memory (not provable to be alias free)?
fn clobbers_memory(op: &Op) -> bool {
    matches!(
        op,
        Op::Store { .. }
            | Op::Call { .. }
            | Op::CallIndirect { .. }
            | Op::Syscall { .. }
            | Op::CopyMem { .. }
            | Op::AtomicAdd { .. }
            // ROUND 52: the inline assembler can touch any memory
            // (`clobber("memory")` is the rule, not the exception), and an
            // MMIO write is a side effect by definition.
            | Op::Asm { .. }
            | Op::MmioLoad { .. }
            | Op::MmioStore { .. }
            | Op::AtomicCas { .. }
            | Op::ThreadSpawn { .. }
            | Op::SecureZero { .. }
    )
}

/// `store p, v; ... ; load p` -> `v` and `load p; ...; load p` -> first value,
/// each only within one block and only without a memory effect between
/// them. Yields the number of forwarded `load`s.
pub(crate) fn forward_local_loads(f: &mut Func) -> usize {
    let mut map: HashMap<Val, Val> = HashMap::new();
    for b in &f.blocks {
        // known cell contents: address value -> (type, value)
        let mut known: HashMap<Val, (crate::fir::FTy, Val)> = HashMap::new();
        for i in &b.insts {
            match &i.op {
                Op::Load { addr } => {
                    if let Some(d) = i.dst {
                        match known.get(addr) {
                            Some(&(t, v)) if t == i.ty && !locked(f, v) && !locked(f, d) => {
                                map.insert(d, v);
                            }
                            _ => {
                                known.insert(*addr, (i.ty, d));
                            }
                        }
                    }
                }
                Op::Store { addr, val } => {
                    // every other entry could mean the same cell
                    known.clear();
                    known.insert(*addr, (i.ty, *val));
                }
                other => {
                    if clobbers_memory(other) {
                        known.clear();
                    }
                }
            }
        }
    }
    if map.is_empty() {
        return 0;
    }
    let n = map.len();
    replace_uses(f, &map);
    n
}

// --------------------------------------- copy propagation / simplification ---

/// Identities and trivial algebraic simplifications. Yields the number of
/// substitutions.
pub(crate) fn copy_propagate(f: &mut Func) -> usize {
    let mut consts: HashMap<Val, i128> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                consts.insert(d, *c);
            }
        }
    }
    let mut map: HashMap<Val, Val> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            let d = match i.dst {
                Some(d) => d,
                None => continue,
            };
            if locked(f, d) || is_untouchable(&i.op) {
                continue;
            }
            let same = match &i.op {
                Op::Cast { src, from } => {
                    // Same width AND same sign: pure reinterpretation of the
                    // same bit pattern (say `usize` <-> `*mut T`).
                    //
                    // `f64` MUST NOT JOIN IN HERE. It is 64 bits wide and
                    // counts as unsigned — by the rule above `u64 -> f64`
                    // therefore looked like a pure reinterpretation, and the
                    // conversion vanished without replacement. `100 as f64`
                    // thereby became the bit pattern 100 rather than the
                    // value 100.0. It is exactly the other way round: of all
                    // conversions the one between integer and floating point
                    // is the only one that REALLY changes the bits (`cvtsi2sd`).
                    //
                    // Found while comparing the lexer written in Firn against
                    // `firnc0` (round 20): `10.0` gave two different token
                    // streams, depending on whether the optimizer ran.
                    //
                    // ROUND 71: `f32 -> f64` is such a conversion as well,
                    // and so is `f32 -> u32` -- same width, same signedness
                    // by the table, completely different bits. The rule is
                    // therefore: as soon as floating point is involved on
                    // ONE side and the types differ, nothing is dropped.
                    let floatswitch = (from.is_float() || i.ty.is_float()) && *from != i.ty;
                    if !floatswitch
                        && (*from == i.ty
                            || (from.bits() == i.ty.bits()
                                && from.signed() == i.ty.signed()
                                && *from != crate::fir::FTy::Bool
                                && i.ty != crate::fir::FTy::Bool))
                    {
                        Some(*src)
                    } else {
                        None
                    }
                }
                Op::PtrAdd { base, off } => {
                    if consts.get(off) == Some(&0) {
                        Some(*base)
                    } else {
                        None
                    }
                }
                Op::Bin(op, a, b2) => {
                    let ca = consts.get(a).copied();
                    let cb = consts.get(b2).copied();
                    let all_ones = |t: crate::fir::FTy| t.truncate(-1);
                    match op {
                        BinOp::Add => {
                            if cb == Some(0) {
                                Some(*a)
                            } else if ca == Some(0) {
                                Some(*b2)
                            } else {
                                None
                            }
                        }
                        BinOp::Sub | BinOp::Shl | BinOp::Shr => {
                            if cb == Some(0) {
                                Some(*a)
                            } else {
                                None
                            }
                        }
                        BinOp::Or | BinOp::Xor => {
                            if cb == Some(0) {
                                Some(*a)
                            } else if ca == Some(0) {
                                Some(*b2)
                            } else {
                                None
                            }
                        }
                        BinOp::Mul | BinOp::Div => {
                            if cb == Some(1) {
                                Some(*a)
                            } else if *op == BinOp::Mul && ca == Some(1) {
                                Some(*b2)
                            } else {
                                None
                            }
                        }
                        BinOp::And => {
                            if cb == Some(all_ones(i.ty)) {
                                Some(*a)
                            } else if ca == Some(all_ones(i.ty)) {
                                Some(*b2)
                            } else {
                                None
                            }
                        }
                        BinOp::Rem => None,
                    }
                }
                _ => None,
            };
            if let Some(s) = same {
                if s != d
                    && !locked(f, s)
                    && f.val_ty(s).bits() == f.val_ty(d).bits()
                    && f.val_ty(s).signed() == f.val_ty(d).signed()
                {
                    map.insert(d, s);
                }
            }
        }
    }
    if map.is_empty() {
        return 0;
    }
    // Resolve chains (a->b->c), but without cycle risk.
    let keys: Vec<Val> = map.keys().copied().collect();
    for k in keys {
        let mut cur = map[&k];
        let mut steps = 0;
        while let Some(&next) = map.get(&cur) {
            if next == cur || steps > 64 {
                break;
            }
            cur = next;
            steps += 1;
        }
        map.insert(k, cur);
    }
    let n = map.len();
    replace_uses(f, &map);
    n
}

// ------------------------------------------------------------- block merging ---

/// Merges `A -> B` when A has exactly one successor (B) and B exactly one
/// predecessor (A), and bridges empty `br` blocks.
/// Yields the number of removed blocks.
pub(crate) fn merge_blocks(f: &mut Func) -> usize {
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let mut removed = 0usize;
    // (1) jump threading: an empty block with `br C` is skipped.
    let mut rounds = 0;
    loop {
        rounds += 1;
        let n = f.blocks.len();
        let mut redirect: Vec<Option<u32>> = vec![None; n];
        for (i, b) in f.blocks.iter().enumerate() {
            if i == 0 || !b.insts.is_empty() {
                continue;
            }
            if let Term::Br(t) = b.term {
                if t as usize != i {
                    redirect[i] = Some(t);
                }
            }
        }
        if redirect.iter().all(|r| r.is_none()) {
            break;
        }
        // resolve chains (with a cap against cycles)
        let resolve = |mut t: u32| -> u32 {
            let mut steps = 0;
            while let Some(nt) = redirect[t as usize] {
                if nt == t || steps > 64 {
                    break;
                }
                t = nt;
                steps += 1;
            }
            t
        };
        let mut changed = false;
        let mut new_terms: Vec<Term> = Vec::with_capacity(n);
        for b in f.blocks.iter() {
            let t = match &b.term {
                Term::Br(t) => {
                    let r = resolve(*t);
                    if r != *t {
                        changed = true;
                    }
                    Term::Br(r)
                }
                Term::BrCond { cond, then_bb, else_bb } => {
                    let (a, b2) = (resolve(*then_bb), resolve(*else_bb));
                    if a != *then_bb || b2 != *else_bb {
                        changed = true;
                    }
                    Term::BrCond { cond: *cond, then_bb: a, else_bb: b2 }
                }
                Term::Switch { val, ty, cases, default } => {
                    let cs: Vec<(i128, u32)> = cases.iter().map(|(k, t)| (*k, resolve(*t))).collect();
                    let d = resolve(*default);
                    if cs != *cases || d != *default {
                        changed = true;
                    }
                    Term::Switch { val: *val, ty: *ty, cases: cs, default: d }
                }
                other => other.clone(),
            };
            new_terms.push(t);
        }
        if !changed {
            break;
        }
        for (b, t) in f.blocks.iter_mut().zip(new_terms) {
            b.term = t;
        }
        removed += 1;
        if rounds > 16 {
            break;
        }
    }

    // (2) merging: A ends with `br B`, B has A as its only predecessor.
    //
    // ROUND 87 -- THE QUADRATIC LOOP.
    //
    // This used to recompute the reachability AND the whole predecessor
    // table from scratch for EVERY SINGLE merged block, find exactly one
    // pair, merge it, and start again. A function with a hundred mergeable
    // blocks paid a hundred passes over its own control flow graph, each of
    // them with fresh allocations. Measured over bin/firnc1.fi: 628
    // productive calls of this pass cost 670 of the optimizer's 3,460 ms --
    // 1.07 ms each, for a pass that copies instruction lists around.
    //
    // Both tables are now built ONCE and kept up to date by hand. Merging A
    // and B changes exactly two things: B becomes unreachable, and wherever
    // B was a predecessor, A now stands. The update is a walk over the
    // successors of the terminator that has just moved.
    //
    // The update may be TOO COARSE in one place, and deliberately so: if a
    // block s had both A and B as predecessors, `pr[s]` afterwards holds A
    // twice, and `pr[s].len() == 1` is then false although there is really
    // only one predecessor left. That prevents a merge, it never causes a
    // wrong one -- and the fixpoint loop in opt.rs calls this pass again with
    // freshly built tables, which catches it.
    let n0 = f.blocks.len();
    let mut reach = vec![false; n0];
    let mut stack = vec![0usize];
    if !reach.is_empty() {
        reach[0] = true;
    }
    while let Some(bi) = stack.pop() {
        for sblk in f.blocks[bi].term.successors() {
            let sblk = sblk as usize;
            if sblk < reach.len() && !reach[sblk] {
                reach[sblk] = true;
                stack.push(sblk);
            }
        }
    }
    let mut pr = preds(f);
    for p in pr.iter_mut() {
        p.retain(|&x| reach[x]);
    }
    let mut scan = 0usize;
    let mut rounds = 0;
    loop {
        rounds += 1;
        let mut target: Option<(usize, usize)> = None;
        while scan < f.blocks.len() {
            let i = scan;
            scan += 1;
            if !reach[i] {
                continue;
            }
            if let Term::Br(t) = f.blocks[i].term {
                let t = t as usize;
                if t != i && t != 0 && t < f.blocks.len() && pr[t].len() == 1 && pr[t][0] == i {
                    // Allocas may stand in the entry block only: when merging
                    // into bb0 that holds, otherwise only when B contains no
                    // alloca.
                    let has_alloca =
                        f.blocks[t].insts.iter().any(|x| matches!(x.op, Op::Alloca { .. }));
                    if has_alloca && i != 0 {
                        continue;
                    }
                    target = Some((i, t));
                    break;
                }
            }
        }
        let (a, b) = match target {
            Some(x) => x,
            None => break,
        };
        let moved = std::mem::take(&mut f.blocks[b].insts);
        let term = f.blocks[b].term.clone();
        if a == 0 {
            // Allocas have to stay at the front.
            let (allocas, rest): (Vec<Inst>, Vec<Inst>) =
                moved.into_iter().partition(|x| matches!(x.op, Op::Alloca { .. }));
            let pos = f.blocks[0]
                .insts
                .iter()
                .take_while(|x| matches!(x.op, Op::Alloca { .. }))
                .count();
            for (k, ins) in allocas.into_iter().enumerate() {
                f.blocks[0].insts.insert(pos + k, ins);
            }
            f.blocks[0].insts.extend(rest);
        } else {
            f.blocks[a].insts.extend(moved);
        }
        // B's successors now have A as their predecessor instead of B.
        for sblk in term.successors() {
            let sblk = sblk as usize;
            if sblk < pr.len() {
                for x in pr[sblk].iter_mut() {
                    if *x == b {
                        *x = a;
                    }
                }
            }
        }
        f.blocks[a].term = term;
        f.blocks[b].term = Term::Unset; // becomes unreachable -> DCE cleans up
        f.blocks[b].insts.clear();
        reach[b] = false;
        pr[b].clear();
        // A has taken over B's terminator, so A itself may now be mergeable
        // with B's successor: look at A again.
        scan = a;
        removed += 1;
        if rounds > 4096 {
            break;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{CmpOp, FTy, Module, Term};

    #[test]
    fn once_written_alloca_becomes_resolved() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let c = f.push(0, FTy::I32, Op::Const(7));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: c });
        let b1 = f.add_block();
        f.set_term(0, Term::Br(b1));
        let l = f.push(b1, FTy::I32, Op::Load { addr: slot });
        f.set_term(b1, Term::Ret(Some(l)));
        assert_eq!(promote_single_store(&mut f), 1);
        assert!(matches!(f.blocks[1].term, Term::Ret(Some(v)) if v == c));
        // after the complete optimization only the constant is left
        let mut m = Module::new();
        m.funcs.push(f);
        crate::opt::optimize(&mut m);
        assert_eq!(m.funcs[0].inst_count(), 1);
    }

    #[test]
    fn multi_written_alloca_stays() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let c = f.push(0, FTy::I32, Op::Const(1));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: c });
        let b1 = f.add_block();
        let b2 = f.add_block();
        let cond = f.push(0, FTy::Bool, Op::Load { addr: slot });
        f.set_term(0, Term::BrCond { cond, then_bb: b1, else_bb: b2 });
        let c2 = f.push(b1, FTy::I32, Op::Const(2));
        f.push_void(b1, FTy::I32, Op::Store { addr: slot, val: c2 });
        f.set_term(b1, Term::Br(b2));
        let l = f.push(b2, FTy::I32, Op::Load { addr: slot });
        f.set_term(b2, Term::Ret(Some(l)));
        assert_eq!(promote_single_store(&mut f), 0);
    }

    #[test]
    fn load_after_store_becomes_in_block_forwarded() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let c = f.push(0, FTy::I32, Op::Const(5));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: c });
        let l = f.push(0, FTy::I32, Op::Load { addr: slot });
        let s = f.push(0, FTy::I32, Op::Bin(BinOp::Add, l, l));
        f.set_term(0, Term::Ret(Some(s)));
        assert_eq!(forward_local_loads(&mut f), 1);
        assert!(matches!(f.blocks[0].insts.last().unwrap().op, Op::Bin(BinOp::Add, x, y) if x == c && y == c));
    }

    #[test]
    fn call_between_store_and_load_prevents_forwarding() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let c = f.push(0, FTy::I32, Op::Const(5));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: c });
        f.push_void(0, FTy::Void, Op::Call { name: "g".into(), args: vec![] });
        let l = f.push(0, FTy::I32, Op::Load { addr: slot });
        f.set_term(0, Term::Ret(Some(l)));
        assert_eq!(forward_local_loads(&mut f), 0);
    }

    #[test]
    fn algebraic_identities() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let p = f.push(0, FTy::I32, Op::Call { name: "g".into(), args: vec![] });
        let z = f.push(0, FTy::I32, Op::Const(0));
        let a = f.push(0, FTy::I32, Op::Bin(BinOp::Add, p, z));
        let one = f.push(0, FTy::I32, Op::Const(1));
        let b = f.push(0, FTy::I32, Op::Bin(BinOp::Mul, a, one));
        f.set_term(0, Term::Ret(Some(b)));
        assert!(copy_propagate(&mut f) >= 1);
        let mut m = Module::new();
        m.funcs.push(f);
        crate::opt::optimize(&mut m);
        // what is left is only the call (impure) and `ret %call`
        assert!(matches!(m.funcs[0].blocks[0].term, Term::Ret(Some(v)) if v == p));
    }

    #[test]
    fn empty_blocks_become_merged() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let b1 = f.add_block();
        let b2 = f.add_block();
        f.set_term(0, Term::Br(b1));
        f.set_term(b1, Term::Br(b2));
        let c = f.push(b2, FTy::I32, Op::Const(3));
        f.set_term(b2, Term::Ret(Some(c)));
        assert!(merge_blocks(&mut f) > 0);
        let mut m = Module::new();
        m.funcs.push(f);
        crate::opt::optimize(&mut m);
        assert_eq!(m.funcs[0].blocks.len(), 1);
        assert!(matches!(m.funcs[0].blocks[0].term, Term::Ret(Some(_))));
    }

    #[test]
    fn secret_values_stay_untouched() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let c = f.push(0, FTy::I32, Op::Const(9));
        f.secret.insert(c);
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: c });
        let l = f.push(0, FTy::I32, Op::Load { addr: slot });
        f.set_term(0, Term::Ret(Some(l)));
        assert_eq!(forward_local_loads(&mut f), 0);
        assert_eq!(promote_single_store(&mut f), 0);
    }

    #[test]
    fn select_stays_select() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let c = f.push(0, FTy::Bool, Op::Const(1));
        let a = f.push(0, FTy::I32, Op::Const(1));
        let z = f.push(0, FTy::I32, Op::Const(0));
        let a2 = f.push(0, FTy::I32, Op::Bin(BinOp::Add, a, z));
        let s = f.push(0, FTy::I32, Op::Select { cond: c, a: a2, b: z });
        f.set_term(0, Term::Ret(Some(s)));
        copy_propagate(&mut f);
        // the select operand did NOT get rewritten
        assert!(matches!(f.blocks[0].insts.last().unwrap().op, Op::Select { a, .. } if a == a2));
        let mut m = Module::new();
        m.funcs.push(f);
        crate::opt::optimize(&mut m);
        assert!(m.funcs[0]
            .blocks
            .iter()
            .any(|b| b.insts.iter().any(|i| matches!(i.op, Op::Select { .. }))));
        let _ = CmpOp::Eq;
    }
}
