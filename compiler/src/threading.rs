// SPDX-License-Identifier: GPL-2.0-only
//! **Jump threading through bool cells** (round 51).
//!
//! INTERFACE (fixed):
//!   `pub(crate) fn thread_bool_cells(f: &mut Func) -> usize`
//!
//! ## Why this pass
//!
//! FIR has **no phi nodes** (fir.rs, invariant). The short circuit operators
//! `&&` and `||` must therefore merge their result through an `alloca`
//! cell. `if c < 0x80 && c != 13` turns into:
//!
//! ```text
//! bbA: %1 = cmp.lt %c, 128 ; store.bool %1, %cell ; brcond %1, bbB, bbJ
//! bbB: %2 = cmp.ne %c, 13  ; store.bool %2, %cell ; br bbJ
//! bbJ: %3 = load.bool %cell ; brcond %3, bbT, bbE
//! ```
//!
//! `mem2reg` cannot resolve this cell — it is written twice, and without
//! phi there is no value that represents both paths. In machine code
//! that costs seven instructions per pass rather than two:
//!
//! ```text
//! setb  %al                       ; produce the bool
//! movzbl %al,%r11d
//! mov   %r11b,-0xae1(%rbp)        ; into the cell
//! test  %r11b,%r11b               ; check it right away
//! jne   bbB
//! jmp   bbJ
//! bbJ:  movzbl -0xae1(%rbp),%r11d ; out of the cell
//!       test %r11b,%r11b
//!       je   bbE
//! ```
//!
//! Measured at the tokenizer benchmark (realweb, callgrind, instruction
//! exact): the patterns "setcc+movzx+store+reload+test+jcc" and
//! "setcc+movzx+store" together **137.0 M of 958.0 M instructions = 14.3 %**.
//!
//! ## What the pass does
//!
//! It threads the edge past the confluence. A **switch block** is a block
//! made of EXACTLY ONE instruction `%v = load.bool %cell` that ends with
//! `brcond %v, T, E`. A predecessor that executes `store.bool %x, %cell`
//! right before its terminator already knows the content of the cell on
//! that edge — so it may jump straight away:
//!
//! * terminator `br J`            ->  `brcond %x, T, E`
//! * terminator `brcond %x, A, J` ->  `brcond %x, A, E`   (on the J edge
//!   `%x` is false, so the switch block would go to E)
//! * terminator `brcond %x, J, B` ->  `brcond %x, T, B`   (mirror image)
//!
//! After that `cmp` sits right before the terminator again, and the existing
//! fusion `cmp`+`jcc` in `regalloc.rs` applies; the rest (dead `store`,
//! unreachable switch block) falls to `mem2reg::remove_dead_stores` and the
//! block cleanup of `opt.rs`.
//!
//! ## Why that is correct
//!
//! * The `store` is the last instruction before the terminator — between it
//!   and the jump **nothing** can change the cell any more. Allowed between
//!   them are only instructions without memory effect (no `store`, `call`,
//!   `syscall`, `copymem`, `atomicadd`, `securezero`).
//! * The cell is an `alloca` whose pointer **does not escape** (`simple`
//!   from `scan_cells`): it serves only as the address of a `load`/`store`.
//!   A foreign write is thereby ruled out.
//! * `%x` is available in the predecessor — it is an operand of its own
//!   `store`. The live range is not extended, it merely ends one
//!   instruction later at the terminator of the SAME block. This pass
//!   therefore does NOT fall into the class of round 40/41 (where a live
//!   range was stretched across `call` boundaries without the register
//!   allocator knowing). Here no new range beyond a block comes about, and
//!   the allocator sees the terminator operand anyway (`Term::BrCond` is
//!   part of its liveness analysis).
//! * `store` and `alloca` stay; only `remove_dead_stores` clears them away,
//!   and only when the cell really is read nowhere any more. The pass is
//!   thereby debug preserving.
//! * SPEC §9.2: secret values (`secret`) and `#[constant_time]` functions
//!   are not touched — a data flow may never be turned into a
//!   jump.
//!
//! Switchable off with `--no-pass=thread-bool`.

use crate::fir::{BlockId, FTy, Func, Op, Term, Val};
use std::collections::HashMap;

/// Does this instruction change memory that we cannot survey?
fn disturbs_memory(op: &Op) -> bool {
    matches!(
        op,
        Op::Store { .. }
            | Op::Call { .. }
            | Op::CallIndirect { .. }
            | Op::Syscall { .. }
            | Op::CopyMem { .. }
            | Op::AtomicAdd { .. }
            | Op::SecureZero { .. }
    )
}

/// One switch block: just `load.bool` from a cell, then `brcond`.
struct Fork {
    cell: Val,
    then: BlockId,
    els: BlockId,
}

/// **ROUND 92** — the cells this pass is about to fold into jumps.
///
/// `mem2reg.rs` asks before it promotes anything. Without that question the
/// two passes race: `mem2reg` runs first in the round, would turn the bool
/// cell into a phi, and the short circuit of `&&`/`||` that this pass has
/// been folding into two jumps since round 74 would silently stop
/// happening — no test would fail, the code would just get longer. So a
/// cell that stands in a fork block is left in memory for exactly one more
/// round; afterwards the fork block is unreachable, this list is empty, and
/// the cell is promoted like any other.
pub(crate) fn fork_cells(f: &Func) -> std::collections::HashSet<Val> {
    let mut out = std::collections::HashSet::new();
    if f.constant_time || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return out;
    }
    for bi in 0..f.blocks.len() {
        if let Some((cell, _, _)) = fork_at(f, bi) {
            out.insert(cell);
        }
    }
    out
}

/// ROUND SPEED — **the one place that decides what a fork is.**
///
/// A fork block is a block made of EXACTLY ONE instruction
/// `%v = load.bool %cell` that ends in `brcond %v, T, E`. On top of round
/// 51's definition this asks one more thing, and it is the question round 92
/// answered for the whole function instead of for the single fork: **neither
/// `T` nor `E` may carry a phi.** Threading makes a predecessor of the fork
/// jump straight into `T` or `E`; a phi there would get an edge nobody has a
/// value for.
///
/// `thread_bool_cells` and `fork_cells` both ask THIS function, so
/// `mem2reg` leaves alone exactly the cells this pass really takes, and no
/// cell falls between the two.
///
/// Yields `(cell, then, else)`.
pub(crate) fn fork_at(f: &Func, bi: usize) -> Option<(Val, BlockId, BlockId)> {
    let b = f.blocks.get(bi)?;
    if b.id == 0 || b.insts.len() != 1 {
        return None; // bb0 carries the allocas; a phi would make it two
    }
    let i = &b.insts[0];
    let (d, addr) = match (i.dst, &i.op) {
        (Some(d), Op::Load { addr }) => (d, *addr),
        _ => return None,
    };
    if i.ty != FTy::Bool || f.is_secret(d) {
        return None;
    }
    let (cond, then, els) = match &b.term {
        Term::BrCond { cond, then_bb, else_bb } => (*cond, *then_bb, *else_bb),
        _ => return None,
    };
    if cond != d {
        return None;
    }
    let n = f.blocks.len();
    if (then as usize) >= n || (els as usize) >= n {
        return None;
    }
    if f.blocks[then as usize].has_phi() || f.blocks[els as usize].has_phi() {
        return None;
    }
    Some((addr, then, els))
}

/// **ROUND SPEED** — the same threading, one step later: through a **phi**.
///
/// Round 9 of the speed round unblocked `mem2reg` for the bool cells of
/// `&&` / `||`, so a cell that `thread_bool_cells` cannot take (because the
/// arms carry phis, or because the shape is not exactly its shape) becomes a
/// phi instead of staying in memory. That is much better than memory and it
/// is still not what the machine wants:
///
/// ```text
/// cmp r15b, 32 / sete al / movzx r14d, al / test r14b, r14b / jnz ...
/// ```
///
/// five instructions where `cmp` + `jcc` is two. The phi is a bool, its
/// incoming values are the comparisons themselves, and the block does
/// nothing but branch on it — so every predecessor already knows the answer
/// on its own edge and can jump past the join.
///
/// ```text
/// bbA: %1 = cmp ...       ; brcond %1, bbJ, bbB
/// bbB: %2 = cmp ...       ; br bbJ
/// bbJ: %3 = phi [bbA %1, bbB %2] ; brcond %3, T, E
/// ```
///
/// becomes `bbA: brcond %1, T, bbB` and `bbB: brcond %2, T, E`.
///
/// The three rules are the ones round 51 wrote down for the cell, with the
/// value read out of the phi entry instead of out of the last `store`:
///
/// * predecessor ends `br J`             -> `brcond v, T, E`
/// * predecessor ends `brcond v, J, X`   -> `brcond v, T, X`
///   (on the J edge `v` is true, so the join would go to T)
/// * predecessor ends `brcond v, X, J`   -> `brcond v, X, E`   (mirror image)
///
/// and only when the phi's entry for that edge IS that same `v` — otherwise
/// the predecessor does not know the answer.
///
/// **Why the new edges are safe.** `T` and `E` must carry no phi: an edge
/// arriving there would need an entry, and no pass may invent the value that
/// travels along an edge that did not exist (round 92). The entry of the
/// redirected predecessor is struck from the join's phi in the same step, so
/// the entry list and the predecessor list stay in step -- `f.verify_phis()`
/// is what would notice, and it runs before every phi elimination.
///
/// The live range of `v` is not extended: it was already an operand of the
/// terminator of its own block, or it becomes one in the same block it was
/// defined in.
pub(crate) fn thread_bool_phis(f: &mut Func) -> usize {
    if f.constant_time || !f.has_phi() {
        return 0;
    }
    let n = f.blocks.len();
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    // ROUND FIRN-LUECKEN -- THE VALUE OF THE JOIN MAY BE READ NOWHERE ELSE.
    //
    // This is the check the round SPEED version was missing, and it cost the
    // fixpoint: `firnc0` built a stage 1 compiler that could not find a
    // single module any more, because `imports_collect` answered "not found"
    // for a file it had just found.
    //
    // The shape, straight out of `demos/.../probe`:
    //
    //     bb3: %21 = phi.bool [bb1 %6, bb2 %3] ; brcond %21, bb5, bb4
    //     bb6: %20 = phi.bool [bb5 %21, bb9 %19] ; brcond %20, bb11, bb10
    //
    // Threading bb3 drops BOTH of its phi entries -- and `%21` is thereby
    // undefined, while the phi of bb6 still names it as the value that
    // travels along the edge bb5 -> bb6. That is the very rule the round
    // wrote down for itself one paragraph further down ("a pass may delete a
    // block's control flow, but not a definition another instruction still
    // names") -- and it was only applied to the corpse block, not to the
    // value the corpse defines. `%found` of a chain of `if !found` is
    // exactly this shape, which is why every module search in the language's
    // own compiler came out false.
    //
    // The remedy is one set: a join may only be threaded if the value of its
    // phi is read by NOTHING but the `brcond` of its own block. That leaves
    // the case the pass was written for -- the short circuit of `&&` / `||`,
    // whose phi is read exactly once -- untouched.
    let mut foreign: std::collections::HashSet<Val> = std::collections::HashSet::new();
    {
        let mut buf: Vec<Val> = Vec::new();
        for b in f.blocks.iter() {
            for i in &b.insts {
                buf.clear();
                i.op.uses(&mut buf);
                for v in buf.iter() {
                    foreign.insert(*v);
                }
            }
            let t = match &b.term {
                Term::BrCond { cond, .. } => Some(*cond),
                Term::Ret(Some(v)) => Some(*v),
                Term::Switch { val, .. } => Some(*val),
                Term::Br(_) | Term::Ret(None) | Term::Unset => None,
            };
            if let Some(v) = t {
                // The one use that is allowed: the terminator of the very
                // block that defines the value -- that is the join itself.
                let own = b.insts.len() == 1 && b.insts[0].dst == Some(v);
                if !own {
                    foreign.insert(v);
                }
            }
        }
    }
    // 1. The joins: exactly one instruction, and it is a bool phi the
    //    terminator branches on.
    let mut joins: Vec<(usize, BlockId, BlockId, Vec<(BlockId, Val)>)> = Vec::new();
    for bi in 0..n {
        let b = &f.blocks[bi];
        if bi == 0 || b.insts.len() != 1 {
            continue;
        }
        let i = &b.insts[0];
        let (d, inc) = match (i.dst, &i.op) {
            (Some(d), Op::Phi { incoming }) => (d, incoming.clone()),
            _ => continue,
        };
        if i.ty != FTy::Bool || f.is_secret(d) {
            continue;
        }
        // ROUND FIRN-LUECKEN: read anywhere else -- another phi, another
        // instruction, another block's terminator -- and threading would
        // leave that reader with a value that has no definition.
        if foreign.contains(&d) {
            continue;
        }
        let (cond, th, el) = match &b.term {
            Term::BrCond { cond, then_bb, else_bb } => (*cond, *then_bb, *else_bb),
            _ => continue,
        };
        if cond != d || th as usize >= n || el as usize >= n {
            continue;
        }
        if th as usize == bi || el as usize == bi {
            continue; // branches to itself: leave it alone
        }
        if f.blocks[th as usize].has_phi() || f.blocks[el as usize].has_phi() {
            continue;
        }
        joins.push((bi, th, el, inc));
    }
    if joins.is_empty() {
        return 0;
    }
    // 2. Rewrite the predecessors. Collected first, applied afterwards --
    //    one predecessor may feed two joins, and a walk that changes what it
    //    reads is how one loses an edge.
    let mut changes: Vec<(usize, Term)> = Vec::new();
    let mut drop_entry: Vec<(usize, BlockId)> = Vec::new();
    for (j, th, el, inc) in &joins {
        for (pred, v) in inc {
            let pi = *pred as usize;
            if pi >= n || pi == *j || f.is_secret(*v) || f.val_ty(*v) != FTy::Bool {
                continue;
            }
            if changes.iter().any(|(k, _)| *k == pi) {
                continue; // already rewritten for another join
            }
            let new = match &f.blocks[pi].term {
                Term::Br(t) if *t as usize == *j => {
                    Some(Term::BrCond { cond: *v, then_bb: *th, else_bb: *el })
                }
                Term::BrCond { cond, then_bb, else_bb }
                    if cond == v && *then_bb as usize == *j && *else_bb as usize != *j =>
                {
                    Some(Term::BrCond { cond: *v, then_bb: *th, else_bb: *else_bb })
                }
                Term::BrCond { cond, then_bb, else_bb }
                    if cond == v && *else_bb as usize == *j && *then_bb as usize != *j =>
                {
                    Some(Term::BrCond { cond: *v, then_bb: *then_bb, else_bb: *el })
                }
                _ => None,
            };
            if let Some(t) = new {
                changes.push((pi, t));
                drop_entry.push((*j, *pred));
            }
        }
    }
    if changes.is_empty() {
        return 0;
    }
    let count = changes.len();
    for (pi, t) in changes {
        f.blocks[pi].term = t;
    }
    for (j, pred) in drop_entry {
        let np = f.blocks[j].phi_count();
        for i in f.blocks[j].insts[..np].iter_mut() {
            if let Op::Phi { incoming } = &mut i.op {
                incoming.retain(|(b, _)| *b != pred);
            }
        }
    }
    // A join whose LAST predecessor was rewritten is dead, and what is left
    // standing there is `%x = phi.bool []` -- a phi with no entries at all.
    // No later pass is prepared for that shape, and it does not stay local:
    // `dce` will not take a block apart while it still sees the block's
    // values read (two dead joins referring to each other are exactly that
    // case), so the corpse travels on into `regalloc`, which finds a block
    // it cannot lay out.
    //
    // The first attempt was to leave the corpse `merge_blocks` leaves --
    // no instructions, `Term::Unset`. THAT IS WRONG, and the way it is
    // wrong is worth the paragraph: it deletes the DEFINITION of `%x`
    // while a (likewise dead) `brcond %x` still names it. Inside the
    // function nobody minds. But `inline.rs` copies a callee value by
    // value, and a value with no defining instruction gets no entry in the
    // remap table -- so the old id travels into the CALLER unchanged and
    // lands on whatever value happens to carry that number there. In
    // `tests/860_thread_basic.fi` that was `%68`, a `u64`, and the
    // verifier caught it as `condition %68 is u64, expected bool`.
    //
    // So the definition stays and only the phi goes. An unreachable block
    // may compute anything at all; `false` is as good an answer as any,
    // and it is one `dce` can then remove in the ordinary way.
    for (j, _, _, _) in &joins {
        let jb = *j;
        for i in f.blocks[jb].insts.iter_mut() {
            let empty = matches!(&i.op, Op::Phi { incoming } if incoming.is_empty());
            if empty {
                i.op = Op::Const(0);
            }
        }
    }
    count
}


pub(crate) fn thread_bool_cells(f: &mut Func) -> usize {
    // SPEC §9.2: in constant-time functions no jump ever comes about here.
    if f.constant_time {
        return 0;
    }
    // ROUND 92 -- and ROUND SPEED, which had to narrow it.
    //
    // This pass REDIRECTS edges: it makes a predecessor jump straight past
    // the fork block into its two arms. A block with a phi would then have
    // a predecessor its entry list never heard of, and no pass can invent
    // the value that travels along a new edge. Round 92 answered that with
    // `if f.has_phi() { return 0; }` and the note that `mem2reg` would take
    // the cells over instead.
    //
    // IT DOES NOT. `mem2reg::promote_allocas` leaves exactly these cells
    // alone (`fork_cells`) so as not to delete this optimization -- so in
    // any function that has BOTH a loop and a `&&`/`||`, the cell was
    // dropped by both: `thread-bool` refused because of the loop counter's
    // phi, `mem2reg` refused because `thread-bool` was going to do it.
    // Measured on `bench/firn/jsonscan.fi` (4.00x behind `rustc -O`, the
    // worst of the suite): every one of the six `||` cells of `scan` stayed
    // in MEMORY, and one comparison cost eight instructions with three
    // memory accesses instead of two:
    //
    //     cmp r10b, 123 / sete al / movzx eax, al
    //     mov qword ptr [rbp-224], rax      <- the value, spilled
    //     mov al, byte ptr [rbp-224]        <- read back
    //     mov byte ptr [rbp-3001], al       <- into the cell
    //     cmp byte ptr [rbp-224], 0 / jnz
    //
    // The condition that really matters is not "does this function have a
    // phi" but "does the block I am about to jump into have one" -- and
    // that is asked per fork, in `fork_at`, which `fork_cells` asks as
    // well, so the two passes agree on which cells belong to whom.
    // Invariant blocks[i].id == i — otherwise the indices compute wrong.
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let simple = simple_cells(f);
    if simple.is_empty() {
        return 0;
    }

    // 1. Collect the switch blocks.
    let mut forks: HashMap<BlockId, Fork> = HashMap::new();
    for b in &f.blocks {
        if b.id == 0 || b.insts.len() != 1 {
            continue; // bb0 carries the allocas
        }
        let (addr, then, els) = match fork_at(f, b.id as usize) {
            Some(x) => x,
            None => continue,
        };
        if !simple.contains(&addr) {
            continue;
        }
        forks.insert(b.id, Fork { cell: addr, then, els });
    }
    if forks.is_empty() {
        return 0;
    }

    // 2. Rewrite the predecessors.
    let mut n = 0usize;
    for pi in 0..f.blocks.len() {
        let p = &f.blocks[pi];
        // Which switch block is a successor at all?
        let targets = p.term.successors();
        if !targets.iter().any(|z| forks.contains_key(z)) {
            continue;
        }
        // The cell content written last at the end of the block.
        let (cell, x) = match last_bool_store(f, pi) {
            Some(v) => v,
            None => continue,
        };
        if !simple.contains(&cell) || f.is_secret(x) || f.val_ty(x) != FTy::Bool {
            continue;
        }
        let new = match &f.blocks[pi].term {
            Term::Br(t) => match forks.get(t) {
                Some(w) if w.cell == cell && *t != pi as BlockId => {
                    Some(Term::BrCond { cond: x, then_bb: w.then, else_bb: w.els })
                }
                _ => None,
            },
            Term::BrCond { cond, then_bb, else_bb } if *cond == x => {
                let nt = match forks.get(then_bb) {
                    Some(w) if w.cell == cell && *then_bb != pi as BlockId => w.then,
                    _ => *then_bb,
                };
                let ne = match forks.get(else_bb) {
                    Some(w) if w.cell == cell && *else_bb != pi as BlockId => w.els,
                    _ => *else_bb,
                };
                if nt == *then_bb && ne == *else_bb {
                    None
                } else {
                    Some(Term::BrCond { cond: x, then_bb: nt, else_bb: ne })
                }
            }
            _ => None,
        };
        if let Some(t) = new {
            f.blocks[pi].term = t;
            n += 1;
        }
    }
    n
}

/// `alloca`s whose pointer does NOT escape (address of `load`/`store` only).
fn simple_cells(f: &Func) -> std::collections::HashSet<Val> {
    use std::collections::HashSet;
    let mut cells: HashSet<Val> = HashSet::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { .. }) = (i.dst, &i.op) {
                cells.insert(d);
            }
        }
    }
    if cells.is_empty() {
        return cells;
    }
    let mut out: HashSet<Val> = HashSet::new();
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                // The address of an access is allowed; the STORED value
                // would be a pointer that escapes.
                Op::Load { .. } => {}
                Op::Store { val, .. } => {
                    if cells.contains(val) {
                        out.insert(*val);
                    }
                }
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    for v in &buf {
                        if cells.contains(v) {
                            out.insert(*v);
                        }
                    }
                }
            }
        }
        match &b.term {
            Term::Ret(Some(v)) | Term::BrCond { cond: v, .. } | Term::Switch { val: v, .. } => {
                if cells.contains(v) {
                    out.insert(*v);
                }
            }
            _ => {}
        }
    }
    for v in out {
        cells.remove(&v);
    }
    cells.retain(|v| !f.is_secret(*v));
    cells
}

/// The bool value guaranteed to sit in a cell by the end of block `pi`:
/// the last `store.bool` that is followed by no memory effect up to the
/// terminator. Yields `(cell, value)`.
fn last_bool_store(f: &Func, pi: usize) -> Option<(Val, Val)> {
    let insts = &f.blocks[pi].insts;
    for i in insts.iter().rev() {
        match &i.op {
            Op::Store { addr, val } => {
                if i.ty != FTy::Bool {
                    return None; // foreign write between them
                }
                return Some((*addr, *val));
            }
            op if disturbs_memory(op) => return None,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{CmpOp, Module};

    /// `a < b && a != c` — exactly the form that the decoder produces.
    /// bb0 = entry, bb1 = right side, bb2 = switch, bb3/bb4 = targets.
    fn and_func() -> Func {
        let mut f = Func::new("t", vec![FTy::U32, FTy::U32, FTy::U32], FTy::U32);
        let bb_b = f.add_block();
        let bb_j = f.add_block();
        let bb_t = f.add_block();
        let bb_e = f.add_block();
        let cell = f.alloca(1, 1);
        let c1 = f.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::U32, a: 0, b: 1 });
        f.push_void(0, FTy::Bool, Op::Store { addr: cell, val: c1 });
        f.set_term(0, Term::BrCond { cond: c1, then_bb: bb_b, else_bb: bb_j });
        let c2 = f.push(bb_b, FTy::Bool, Op::Cmp { op: CmpOp::Ne, ty: FTy::U32, a: 0, b: 2 });
        f.push_void(bb_b, FTy::Bool, Op::Store { addr: cell, val: c2 });
        f.set_term(bb_b, Term::Br(bb_j));
        let l = f.push(bb_j, FTy::Bool, Op::Load { addr: cell });
        f.set_term(bb_j, Term::BrCond { cond: l, then_bb: bb_t, else_bb: bb_e });
        let one = f.push(bb_t, FTy::U32, Op::Const(1));
        f.set_term(bb_t, Term::Ret(Some(one)));
        let null = f.push(bb_e, FTy::U32, Op::Const(0));
        f.set_term(bb_e, Term::Ret(Some(null)));
        f
    }

    #[test]
    fn and_short_circuit_becomes_threaded() {
        let mut f = and_func();
        let n = thread_bool_cells(&mut f);
        assert_eq!(n, 2, "both predecessors of the branch must be threaded");
        match &f.blocks[0].term {
            Term::BrCond { then_bb, else_bb, .. } => {
                assert_eq!(*then_bb, 1);
                assert_eq!(*else_bb, 4, "wrong edge goes straight to bb_e");
            }
            t => panic!("bb0: {:?}", t),
        }
        match &f.blocks[1].term {
            Term::BrCond { then_bb, else_bb, .. } => {
                assert_eq!(*then_bb, 3);
                assert_eq!(*else_bb, 4);
            }
            t => panic!("bb1: {:?}", t),
        }
        // The switch block itself stays unchanged (the block
        // cleanup in opt.rs clears it away later).
        assert_eq!(f.blocks[2].insts.len(), 1);
    }

    #[test]
    fn second_run_changes_nothing_more() {
        let mut f = and_func();
        assert_eq!(thread_bool_cells(&mut f), 2);
        assert_eq!(thread_bool_cells(&mut f), 0, "fixed point after one run");
    }

    #[test]
    fn cell_the_escapes_becomes_not_threaded() {
        let mut f = and_func();
        let cell = 3; // %0..%2 are parameters, %3 the alloca
        f.push_void(3, FTy::Void, Op::Call { name: "foreign".into(), args: vec![cell] });
        assert!(!simple_cells(&f).contains(&cell));
        assert_eq!(thread_bool_cells(&mut f), 0);
    }

    #[test]
    fn call_between_store_and_jump_blocked() {
        let mut f = and_func();
        f.push_void(1, FTy::Void, Op::Call { name: "foreign".into(), args: vec![] });
        // bb1 now has a call BEHIND the store — threading is forbidden
        // there, but allowed for bb0.
        assert_eq!(thread_bool_cells(&mut f), 1);
        assert!(matches!(f.blocks[1].term, Term::Br(2)));
    }

    #[test]
    fn foreign_store_between_store_and_jump_blocked() {
        let mut f = and_func();
        let p = f.push(1, FTy::Ptr, Op::Const(0));
        let w = f.push(1, FTy::U64, Op::Const(7));
        f.push_void(1, FTy::U64, Op::Store { addr: p, val: w });
        assert_eq!(thread_bool_cells(&mut f), 1);
        assert!(matches!(f.blocks[1].term, Term::Br(2)));
    }

    #[test]
    fn constant_time_stays_untouched() {
        let mut f = and_func();
        f.constant_time = true;
        assert_eq!(thread_bool_cells(&mut f), 0);
    }

    #[test]
    fn secret_value_stays_untouched() {
        let mut f = and_func();
        let c1 = 4; // %0..%2 parameters, %3 = alloca, %4 = cmp.lt
        f.secret.insert(c1);
        // Only the predecessor with the secret value stays.
        assert_eq!(thread_bool_cells(&mut f), 1);
        match &f.blocks[0].term {
            Term::BrCond { then_bb, else_bb, .. } => {
                assert_eq!((*then_bb, *else_bb), (1, 2), "bb0 unchanged");
            }
            t => panic!("bb0: {:?}", t),
        }
    }

    #[test]
    fn module_stays_compilable() {
        let mut m = Module::default();
        m.funcs.push(and_func());
        for f in m.funcs.iter_mut() {
            thread_bool_cells(f);
        }
        assert_eq!(m.funcs.len(), 1);
    }
}
