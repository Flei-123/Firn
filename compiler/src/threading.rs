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

pub(crate) fn thread_bool_cells(f: &mut Func) -> usize {
    // SPEC §9.2: in constant-time functions no jump ever comes about here.
    if f.constant_time {
        return 0;
    }
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
        let i = &b.insts[0];
        let (d, addr) = match (i.dst, &i.op) {
            (Some(d), Op::Load { addr }) => (d, *addr),
            _ => continue,
        };
        if i.ty != FTy::Bool || !simple.contains(&addr) || f.is_secret(d) {
            continue;
        }
        let (cond, then, els) = match &b.term {
            Term::BrCond { cond, then_bb, else_bb } => (*cond, *then_bb, *else_bb),
            _ => continue,
        };
        if cond != d {
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
