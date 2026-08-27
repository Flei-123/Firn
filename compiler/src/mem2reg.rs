// SPDX-License-Identifier: GPL-2.0-only
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

use crate::fir::{BinOp, FTy, Func, Inst, Op, Term, Val};
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
                // ROUND 72: same shape as `Op::Bin` — two operands, no
                // special casing needed for copy propagation.
                Op::BinWrapSat { a, b: b2, .. }
                | Op::CheckedBin { a, b: b2, .. }
                | Op::CheckedDiv { a, b: b2, .. } => {
                    rep(a, &mut n);
                    rep(b2, &mut n);
                }
                Op::CheckedCast { src, .. } => rep(src, &mut n),
                Op::CheckedIdx { idx, .. } => rep(idx, &mut n),
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
                Op::VtabAddr { .. } | Op::FnRef { .. } | Op::GlobalAddr { .. } => {}
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
                // ROUND 92: a phi operand is an ordinary use and gets
                // rewritten like every other one. The BLOCK numbers next to
                // them are control flow, not values -- they are never
                // touched here.
                Op::Phi { incoming } => {
                    for (_, v) in incoming.iter_mut() {
                        rep(v, &mut n);
                    }
                }
                Op::Copy { src } => rep(src, &mut n),
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

// ------------------------------------------------ ROUND 92: SSA construction ---
//
// WHY THIS SECTION EXISTS, and it is the whole of round 92.
//
// Until this round `mem2reg` could resolve exactly one shape of `alloca`:
// written ONCE, with the `store` dominating every `load`. Anything written a
// second time stayed in MEMORY, because FIR had no way of saying "the value
// here is the one from the loop body on the back edge and the one from the
// preheader on the first pass". Every loop counter is written on every pass.
// So every loop counter stayed in memory, and with it every derived address:
// no induction variables, no range analysis across a back edge, no loop
// invariant motion of anything that reads the counter.
//
// What is built here is the textbook answer, Cytron/Ferrante/Rosen/Wegman/
// Zadeck 1991, in the three parts it has always had:
//
//   1. the DOMINATOR TREE           -- `idoms`
//   2. the DOMINANCE FRONTIERS      -- `dom_frontiers`, and the phis go
//      exactly there: block y needs a phi for variable v when two different
//      definitions of v can reach y, and that is precisely y in DF(x) for a
//      block x defining v
//   3. the RENAMING                 -- one walk over the dominator tree with
//      a stack per variable; a `load` becomes the top of the stack, a
//      `store` pushes, and the phi entries of the successors are filled from
//      the tops as the walk leaves a block
//
// WHY NOT THE OLD `dominators()`. That one returns an n x n matrix of
// `bool`, and `bin/firnc1.fi` has functions with several thousand blocks --
// the matrix alone would be megabytes per function. `idoms` is the iterative
// algorithm of Cooper/Harvey/Kennedy: one `u32` per block, a reverse
// postorder, and an intersection that walks two chains upwards. Same answer,
// linear memory.
//
// WHAT IS DELIBERATELY NOT PROMOTED
//
//   * anything that is not `simple` -- the address leaves as a value
//   * mixed access widths (`store` 8 octets, `load` 4): that is memory
//     semantics, not a variable
//   * `secret` values and everything an untouchable instruction reads
//     (SPEC 9.2: a `select`'s operands are never rewritten, so a value it
//     reads may never be deleted out from under it)
//   * `v128` -- no backend has a phi-capable vector path yet
//   * the bool cells that `threading.rs` is about to fold into jumps this
//     very round; see `fork_cells` there

/// Immediate dominators, plus the reverse postorder they were computed in.
///
/// `idom[b] == b` for the entry block and for every block the entry cannot
/// reach; `rpo_num[b] == usize::MAX` marks unreachable.
pub(crate) struct DomTree {
    pub idom: Vec<u32>,
    pub rpo: Vec<usize>,
    pub rpo_num: Vec<usize>,
    pub preds: Vec<Vec<usize>>,
}

pub(crate) fn idoms(f: &Func) -> DomTree {
    let n = f.blocks.len();
    let succ: Vec<Vec<u32>> = f.blocks.iter().map(|b| b.term.successors()).collect();
    // depth first postorder from the entry, iterative (a recursive walk dies
    // on `gctext__gctext_write`).
    let mut post: Vec<usize> = Vec::with_capacity(n);
    let mut seen = vec![false; n];
    let mut stack: Vec<(usize, usize)> = Vec::with_capacity(64);
    if n > 0 {
        seen[0] = true;
        stack.push((0, 0));
    }
    while let Some((b, k)) = stack.pop() {
        if k < succ[b].len() {
            stack.push((b, k + 1));
            let s = succ[b][k] as usize;
            if s < n && !seen[s] {
                seen[s] = true;
                stack.push((s, 0));
            }
        } else {
            post.push(b);
        }
    }
    let rpo: Vec<usize> = post.into_iter().rev().collect();
    let mut rpo_num = vec![usize::MAX; n];
    for (i, &b) in rpo.iter().enumerate() {
        rpo_num[b] = i;
    }
    let pr = preds(f);
    let mut idom = vec![u32::MAX; n];
    if n > 0 {
        idom[0] = 0;
    }
    // The intersection of two dominator chains: climb the one that sits
    // deeper in the reverse postorder until both stand at the same block.
    let intersect = |mut a: usize, mut b: usize, idom: &[u32], rpo_num: &[usize]| -> usize {
        let mut guard = 0;
        while a != b {
            guard += 1;
            if guard > 4 * n + 16 {
                return a; // cannot happen; never spin
            }
            while rpo_num[a] > rpo_num[b] {
                let nx = idom[a] as usize;
                if nx == a {
                    break;
                }
                a = nx;
            }
            while rpo_num[b] > rpo_num[a] {
                let nx = idom[b] as usize;
                if nx == b {
                    break;
                }
                b = nx;
            }
            if rpo_num[a] == rpo_num[b] && a != b {
                return a;
            }
        }
        a
    };
    let mut rounds = 0;
    loop {
        rounds += 1;
        let mut changed = false;
        for &b in rpo.iter().skip(1) {
            let mut new_idom = usize::MAX;
            for &p in &pr[b] {
                if rpo_num[p] == usize::MAX || idom[p] == u32::MAX {
                    continue; // unreachable predecessor: contributes nothing
                }
                new_idom = if new_idom == usize::MAX {
                    p
                } else {
                    intersect(p, new_idom, &idom, &rpo_num)
                };
            }
            if new_idom != usize::MAX && idom[b] as usize != new_idom {
                idom[b] = new_idom as u32;
                changed = true;
            }
        }
        if !changed || rounds > n + 4 {
            break;
        }
    }
    for b in 0..n {
        if idom[b] == u32::MAX {
            idom[b] = b as u32; // unreachable: its own root
        }
    }
    DomTree { idom, rpo, rpo_num, preds: pr }
}

/// The dominance frontier of every block (Cytron et al., figure 10).
///
/// `y` lies in `DF(x)` when `x` dominates a predecessor of `y` but does not
/// strictly dominate `y` itself -- the first place at which a definition
/// made in `x` meets one made somewhere else.
pub(crate) fn dom_frontiers(dt: &DomTree) -> Vec<Vec<u32>> {
    let n = dt.idom.len();
    let mut df: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut mark: Vec<usize> = vec![usize::MAX; n];
    for b in 0..n {
        if dt.rpo_num[b] == usize::MAX || dt.preds[b].len() < 2 {
            continue;
        }
        for &p in &dt.preds[b] {
            if dt.rpo_num[p] == usize::MAX {
                continue;
            }
            let mut runner = p;
            let mut guard = 0;
            while runner != dt.idom[b] as usize {
                guard += 1;
                if guard > n + 4 {
                    break;
                }
                if mark[runner] != b {
                    mark[runner] = b;
                    df[runner].push(b as u32);
                }
                let nx = dt.idom[runner] as usize;
                if nx == runner {
                    break;
                }
                runner = nx;
            }
        }
    }
    df
}

/// Values that an UNTOUCHABLE instruction reads (SPEC 9.2). `replace_uses`
/// deliberately does not rewrite those operands -- so a value one of them
/// reads must never be deleted, and a cell whose `load` feeds one is not
/// promotable.
fn untouchable_uses(f: &Func) -> std::collections::HashSet<Val> {
    let mut out = std::collections::HashSet::new();
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            if is_untouchable(&i.op) {
                buf.clear();
                i.op.uses(&mut buf);
                out.extend(buf.iter().copied());
            }
        }
    }
    out
}

/// Follows a substitution chain `a -> b -> c` to its end.
fn chase(map: &HashMap<Val, Val>, mut v: Val) -> Val {
    let mut steps = 0;
    while let Some(&nx) = map.get(&v) {
        if nx == v || steps > 64 {
            break;
        }
        v = nx;
        steps += 1;
    }
    v
}

/// **THE PASS.** Turns every promotable `alloca` into values, with real phi
/// nodes where paths meet. Yields the number of `load`s resolved.
pub(crate) fn promote_allocas(f: &mut Func) -> usize {
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    // The function is entered through an edge that has no block behind it,
    // so a phi in the entry block could never be given a copy (`phi.rs`).
    // The lowering never builds such a graph -- bb0 holds the allocas and
    // falls through into whatever comes next -- but a pass could, and the
    // price of asking is one walk over the terminators.
    if f.blocks.iter().any(|b| b.term.successors().contains(&0)) {
        return 0;
    }
    let cells = scan_cells(f);
    if cells.is_empty() {
        return 0;
    }
    let untouchable = untouchable_uses(f);
    let forks = crate::threading::fork_cells(f);

    // ---- 1. which cells may become values -------------------------------
    // Sorted by value id: a `HashMap` walk is not an order, and two runs of
    // the compiler have to produce the same text (tools/fixpoint.sh).
    let mut list: Vec<(Val, FTy)> = Vec::new();
    let mut keys: Vec<Val> = cells.keys().copied().collect();
    keys.sort_unstable();
    for cell in keys {
        let u = &cells[&cell];
        if !u.simple || u.loads.is_empty() || u.stores.is_empty() || locked(f, cell) {
            continue;
        }
        // ROUND 92: `threading.rs` folds a bool cell that is read in a block
        // of its own into the jumps of its predecessors -- the short circuit
        // of `&&` and `||`. That only works while the cell is still THERE.
        // Promoting it first would silently delete an optimization that has
        // been in the compiler since round 74, so those cells are left alone
        // for this round of the fixpoint; once `thread-bool` has rewritten
        // the jumps the read block is unreachable, the fork is gone, and the
        // next round promotes the cell like any other.
        if forks.contains(&cell) {
            continue;
        }
        let mut ty: Option<FTy> = None;
        let mut ok = true;
        for &(bi, ii) in u.loads.iter().chain(u.stores.iter()) {
            let inst = &f.blocks[bi].insts[ii];
            match ty {
                None => ty = Some(inst.ty),
                Some(t) if t == inst.ty => {}
                _ => {
                    ok = false; // mixed widths: memory semantics, hands off
                    break;
                }
            }
        }
        let ty = match (ok, ty) {
            (true, Some(t)) => t,
            _ => continue,
        };
        if ty == FTy::Void || ty == FTy::V128 {
            continue;
        }
        for &(bi, ii) in &u.loads {
            if let Some(d) = f.blocks[bi].insts[ii].dst {
                if locked(f, d) || untouchable.contains(&d) {
                    ok = false;
                    break;
                }
            }
        }
        for &(bi, ii) in &u.stores {
            if let Op::Store { val, .. } = &f.blocks[bi].insts[ii].op {
                if locked(f, *val) {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            list.push((cell, ty));
        }
    }
    if list.is_empty() {
        return 0;
    }
    let ncell = list.len();
    let mut idx_of: HashMap<Val, usize> = HashMap::new();
    for (k, (c, _)) in list.iter().enumerate() {
        idx_of.insert(*c, k);
    }

    // ---- 2. dominator tree, dominance frontiers -------------------------
    let dt = idoms(f);
    let df = dom_frontiers(&dt);
    let nb = f.blocks.len();

    // ---- 3. phi placement ------------------------------------------------
    // `phi_of[b][k]` = the value the phi for cell k defines in block b.
    let mut phi_of: Vec<HashMap<usize, Val>> = vec![HashMap::new(); nb];
    let mut defs: Vec<Vec<usize>> = vec![Vec::new(); ncell];
    for (k, (cell, _)) in list.iter().enumerate() {
        let u = &cells[cell];
        for &(bi, _) in &u.stores {
            if !defs[k].contains(&bi) {
                defs[k].push(bi);
            }
        }
    }
    // New phi instructions, collected per block and inserted in ONE go, in
    // ascending cell order -- again for determinism.
    let mut new_phis: Vec<Vec<(usize, Val)>> = vec![Vec::new(); nb];
    for k in 0..ncell {
        let ty = list[k].1;
        let mut work: Vec<usize> = defs[k].clone();
        let mut on_list: Vec<bool> = vec![false; nb];
        for &b in &work {
            on_list[b] = true;
        }
        let mut has: Vec<bool> = vec![false; nb];
        let mut guard = 0usize;
        while let Some(x) = work.pop() {
            guard += 1;
            if guard > 8 * nb + 64 {
                break;
            }
            for &y in &df[x] {
                let y = y as usize;
                if has[y] || dt.rpo_num[y] == usize::MAX {
                    continue;
                }
                has[y] = true;
                let v = f.new_val_pub(ty);
                phi_of[y].insert(k, v);
                new_phis[y].push((k, v));
                if !on_list[y] {
                    on_list[y] = true;
                    work.push(y);
                }
            }
        }
    }
    for b in 0..nb {
        if new_phis[b].is_empty() {
            continue;
        }
        new_phis[b].sort_by_key(|(k, _)| *k);
        for (n, (_, v)) in new_phis[b].iter().enumerate() {
            let ty = f.val_ty(*v);
            f.blocks[b]
                .insts
                .insert(n, Inst::new(Some(*v), ty, Op::Phi { incoming: Vec::new() }));
        }
    }

    // ---- 4. one undef per cell, in the entry block -----------------------
    // A `load` that no `store` reaches on its path reads whatever was in the
    // frame. FIR has no `undef`, and inventing one for this single case
    // would be a new invariant for every pass to respect -- a zero of the
    // cell's own type says the same thing and is a value like any other.
    let mut undef: Vec<Option<Val>> = vec![None; ncell];

    // ---- 5. renaming -----------------------------------------------------
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nb];
    for b in 0..nb {
        if b == 0 || dt.rpo_num[b] == usize::MAX {
            continue;
        }
        let p = dt.idom[b] as usize;
        if p != b {
            children[p].push(b);
        }
    }
    for c in children.iter_mut() {
        c.sort_unstable();
    }
    let mut stack: Vec<Vec<Val>> = vec![Vec::new(); ncell];
    let mut pushed: Vec<Vec<usize>> = vec![Vec::new(); nb];
    let mut map: HashMap<Val, Val> = HashMap::new();
    let mut resolved = 0usize;
    // (block, is_exit)
    let mut work: Vec<(usize, bool)> = Vec::new();
    if nb > 0 {
        work.push((0, true));
        work.push((0, false));
    }
    while let Some((b, exit)) = work.pop() {
        if exit {
            for &k in pushed[b].iter().rev() {
                stack[k].pop();
            }
            pushed[b].clear();
            continue;
        }
        for (k, v) in new_phis[b].iter() {
            stack[*k].push(*v);
            pushed[b].push(*k);
        }
        let insts = std::mem::take(&mut f.blocks[b].insts);
        let mut keep: Vec<Inst> = Vec::with_capacity(insts.len());
        for inst in insts {
            match &inst.op {
                Op::Alloca { .. } => {
                    if inst.dst.map(|d| idx_of.contains_key(&d)).unwrap_or(false) {
                        continue; // the slot itself goes away
                    }
                    keep.push(inst);
                }
                Op::Load { addr } => match idx_of.get(addr) {
                    Some(&k) => {
                        let cur = match stack[k].last() {
                            Some(&v) => v,
                            None => *undef[k].get_or_insert_with(|| f.new_val_pub(list[k].1)),
                        };
                        if let Some(d) = inst.dst {
                            map.insert(d, cur);
                            resolved += 1;
                        }
                    }
                    None => keep.push(inst),
                },
                Op::Store { addr, val } => match idx_of.get(addr) {
                    Some(&k) => {
                        let rv = chase(&map, *val);
                        stack[k].push(rv);
                        pushed[b].push(k);
                    }
                    None => keep.push(inst),
                },
                _ => keep.push(inst),
            }
        }
        f.blocks[b].insts = keep;
        // fill the phi entries of the successors that this edge feeds
        for s in f.blocks[b].term.successors() {
            let s = s as usize;
            if s >= nb || phi_of[s].is_empty() {
                continue;
            }
            let mut ks: Vec<usize> = phi_of[s].keys().copied().collect();
            ks.sort_unstable();
            for k in ks {
                let cur = match stack[k].last() {
                    Some(&v) => v,
                    None => *undef[k].get_or_insert_with(|| f.new_val_pub(list[k].1)),
                };
                let pv = phi_of[s][&k];
                let np = f.blocks[s].phi_count();
                for i in f.blocks[s].insts[..np].iter_mut() {
                    if i.dst != Some(pv) {
                        continue;
                    }
                    if let Op::Phi { incoming } = &mut i.op {
                        match incoming.iter_mut().find(|(p, _)| *p as usize == b) {
                            Some(e) => e.1 = cur,
                            None => incoming.push((b as u32, cur)),
                        }
                    }
                }
            }
        }
        for &c in children[b].iter().rev() {
            work.push((c, true));
            work.push((c, false));
        }
    }

    // ---- 6. blocks the entry cannot reach --------------------------------
    // They are not in the dominator tree, so the walk above never saw them.
    // Their `load`s still point at an `alloca` that is about to disappear,
    // and they may be listed as predecessors of a block that has a phi. Both
    // are answered with the undef value; the block is dead and `dce` removes
    // it in this same round.
    for b in 0..nb {
        if dt.rpo_num[b] != usize::MAX {
            continue;
        }
        let insts = std::mem::take(&mut f.blocks[b].insts);
        let mut keep: Vec<Inst> = Vec::with_capacity(insts.len());
        for inst in insts {
            let drop = match &inst.op {
                Op::Alloca { .. } => inst.dst.map(|d| idx_of.contains_key(&d)).unwrap_or(false),
                Op::Load { addr } => match idx_of.get(addr) {
                    Some(&k) => {
                        let u = *undef[k].get_or_insert_with(|| f.new_val_pub(list[k].1));
                        if let Some(d) = inst.dst {
                            map.insert(d, u);
                        }
                        true
                    }
                    None => false,
                },
                Op::Store { addr, .. } => idx_of.contains_key(addr),
                _ => false,
            };
            if !drop {
                keep.push(inst);
            }
        }
        f.blocks[b].insts = keep;
        for s in f.blocks[b].term.successors() {
            let s = s as usize;
            if s >= nb || phi_of[s].is_empty() {
                continue;
            }
            let mut ks: Vec<usize> = phi_of[s].keys().copied().collect();
            ks.sort_unstable();
            for k in ks {
                let u = *undef[k].get_or_insert_with(|| f.new_val_pub(list[k].1));
                let pv = phi_of[s][&k];
                let np = f.blocks[s].phi_count();
                for i in f.blocks[s].insts[..np].iter_mut() {
                    if i.dst == Some(pv) {
                        if let Op::Phi { incoming } = &mut i.op {
                            if !incoming.iter().any(|(p, _)| *p as usize == b) {
                                incoming.push((b as u32, u));
                            }
                        }
                    }
                }
            }
        }
    }

    // ---- 7. the undef constants, and the canonical entry order ----------
    for (k, u) in undef.iter().enumerate() {
        if let Some(v) = *u {
            let ty = list[k].1;
            let pos = f.blocks[0]
                .insts
                .iter()
                .take_while(|x| matches!(x.op, Op::Alloca { .. }))
                .count();
            f.blocks[0].insts.insert(pos, Inst::new(Some(v), ty, Op::Const(0)));
        }
    }
    for b in f.blocks.iter_mut() {
        let np = b.phi_count();
        for i in b.insts[..np].iter_mut() {
            if let Op::Phi { incoming } = &mut i.op {
                incoming.sort_by_key(|(p, _)| *p);
            }
        }
    }

    // ---- 8. every `load` that became a value ----------------------------
    if !map.is_empty() {
        let keys: Vec<Val> = map.keys().copied().collect();
        for k in keys {
            let v = chase(&map, k);
            map.insert(k, v);
        }
        replace_uses(f, &map);
    }
    resolved
}

/// **Phi hygiene.** Drops entries for blocks that stopped being predecessors,
/// and folds away a phi that has only one answer left.
///
/// Every pass that removes an edge -- `simplify_terminators` turning a
/// `brcond` with a constant condition into a `br`, `dce` deleting a block --
/// leaves phis behind whose entry list no longer matches the block's
/// predecessors. That is not cosmetic: `phi.rs` puts one copy per
/// PREDECESSOR at the end of the block, and an entry naming a block that is
/// no longer one would put a copy nowhere. So the list is trimmed here,
/// and a phi with a single distinct answer stops being a phi.
///
/// Entries are never INVENTED. A pass that ADDS an edge into a block with a
/// phi would have to say what value travels along it, and no pass in this
/// compiler does that -- `merge_blocks` and `threading.rs` leave such blocks
/// alone on purpose, and `fir::Func::verify_phis` is what catches the day
/// somebody changes that.
pub(crate) fn simplify_phis(f: &mut Func) -> usize {
    if !f.has_phi() {
        return 0;
    }
    let pr = preds(f);
    let mut map: HashMap<Val, Val> = HashMap::new();
    let mut changed = 0usize;
    for (bi, b) in f.blocks.iter_mut().enumerate() {
        let np = b.phi_count();
        for i in b.insts[..np].iter_mut() {
            let d = match i.dst {
                Some(d) => d,
                None => continue,
            };
            if let Op::Phi { incoming } = &mut i.op {
                let before = incoming.len();
                incoming.retain(|(p, _)| pr[bi].contains(&(*p as usize)));
                incoming.sort_by_key(|(p, _)| *p);
                if incoming.len() != before {
                    changed += 1;
                }
                // one answer left (self references do not count as an answer)
                let mut only: Option<Val> = None;
                let mut same = true;
                for (_, v) in incoming.iter() {
                    if *v == d {
                        continue;
                    }
                    match only {
                        None => only = Some(*v),
                        Some(x) if x == *v => {}
                        _ => {
                            same = false;
                            break;
                        }
                    }
                }
                if same {
                    if let Some(v) = only {
                        map.insert(d, v);
                    }
                }
            }
        }
    }
    if map.is_empty() {
        return changed;
    }
    let keys: Vec<Val> = map.keys().copied().collect();
    for k in keys {
        let v = chase(&map, k);
        if v != k {
            map.insert(k, v);
        }
    }
    map.retain(|k, v| k != v);
    changed += map.len();
    replace_uses(f, &map);
    changed
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
    // ROUND 92 -- WHICH BLOCKS CARRY A PHI. Bridging changes WHO the
    // predecessor of a block is, and a phi answers exactly that question per
    // predecessor. Two edges that used to arrive through two different empty
    // blocks may carry two different values; after bridging they arrive from
    // the same predecessor and there is no honest single answer left. So an
    // empty block whose target has phis is not bridged. Phase (2) below
    // still merges it when it has a single predecessor -- that case has one
    // edge and one answer, and there the entry is simply re-keyed.
    let phi_blk: Vec<bool> = f.blocks.iter().map(|b| b.has_phi()).collect();
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
        let resolve = |t0: u32| -> u32 {
            let mut t = t0;
            let mut steps = 0;
            while let Some(nt) = redirect[t as usize] {
                if nt == t || steps > 64 {
                    break;
                }
                t = nt;
                steps += 1;
            }
            if t != t0 && (t as usize) < phi_blk.len() && phi_blk[t as usize] {
                return t0; // see the note above: no honest phi entry
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
                    // ROUND 92: a phi in B would land in the MIDDLE of A
                    // after the merge, and phis stand at the front of a
                    // block. It cannot happen -- a phi is only ever placed
                    // where two paths meet and B has exactly one
                    // predecessor here -- but a merge is not the place to
                    // rely on that.
                    if f.blocks[t].has_phi() {
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
            // ROUND 92 -- AND SO DO THEIR PHI ENTRIES, WITH ONE TRAP IN IT.
            //
            // A's terminator was `br B`, so B was A's ONLY successor: an
            // entry naming A in a block that B jumps to cannot be a live
            // edge, it is a leftover from a `brcond` that `simplify-term`
            // turned into a `br` earlier in this same round. Re-keying B to
            // A without throwing that leftover away gives the phi TWO
            // entries for A, and `phi.rs` then puts two copies of two
            // different values at the end of the same block -- the second
            // one wins, and which one that is depends on the order the
            // entries happen to stand in.
            //
            // Measured: `tests/800_std_str_core.fi` printed nothing at all
            // at `release-safe` and `release-fast` and was right at `dev`
            // and `dev-fast`. `FIRN_VERIFY_PHI=2` named the pass; the entry
            // was `%1585 = phi.bool [bb55 %1582, bb55 %1584]`.
            if sblk < f.blocks.len() {
                let np = f.blocks[sblk].phi_count();
                for i in f.blocks[sblk].insts[..np].iter_mut() {
                    if let Op::Phi { incoming } = &mut i.op {
                        if incoming.iter().any(|(p, _)| *p as usize == b) {
                            incoming.retain(|(p, _)| *p as usize != a);
                        }
                        for e in incoming.iter_mut() {
                            if e.0 as usize == b {
                                e.0 = a as u32;
                            }
                        }
                        incoming.sort_by_key(|(p, _)| *p);
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
