// SPDX-License-Identifier: GPL-2.0-only
//! **LICM** — pull loop invariant computations out of the loop.
//!
//! INTERFACE (fixed):
//!   `pub(crate) fn hoist_loop_invariants(f: &mut Func) -> usize`
//!
//! ## Why this pass
//!
//! Measured on `bench/firn/matmul.fi` (factor 6.26× against Rust, the worst
//! value of the suite): the innermost loop computes
//!
//! ```text
//! s = s + ld32(a, r * n + k) * ld32(b, k * n + cc)
//! ```
//!
//! and `r * n` depends on no loop variable. Before this pass the generated
//! code held one `imul` for it per iteration — 240 times per row,
//! 240 × 240 × 3 times per run. LLVM pulls that out, Firn did not.
//!
//! ## What is hoisted
//!
//! An instruction moves to the **preheader** once all of this holds:
//!
//! * It is **pure**: `const`, `bin`, `cmp`, `un`, `cast`, `ptradd`. No
//!   `load` (not provable without alias analysis), no `store`, `call`,
//!   `syscall`, `copymem`, `alloca`, `gcaddr`.
//! * It **cannot trap**: `div`/`rem` are excluded, because a division by
//!   zero raises a CPU exception. Hoisting would raise it even when the
//!   loop never runs. Shifts are allowed: a width too large is undefined
//!   on x86, but no trap, and the value would be the same throughout the
//!   loop.
//! * **No operand is defined inside the loop** (or it was hoisted itself
//!   already — hence the fixpoint loop).
//! * Neither result nor operand is a `secret` value (SPEC §9.2), and the
//!   instruction is not `select`/`barrier`/`secure_zero`.
//!
//! ## Safety
//!
//! The preheader **dominates** the whole loop; every hoisted definition is
//! therefore valid at every use site it had so far. The `Val` id stays the
//! same, nothing is rewritten — only the position changes.
//!
//! An instruction from a block that does **not** execute on every pass
//! (say inside an `if` in the loop body) executes unconditionally after
//! hoisting. For trap-free, pure computations that preserves behaviour — at
//! worst the preheader computes something nobody reads. Exactly that is why
//! trap freedom above is a condition and not a
//! nicety.
//!
//! Nested loops need no special handling here: `opt.rs` iterates up to the
//! fixpoint, and whatever moved from the inner loop into its preheader lies
//! afterwards in the body of the outer one and moves on in the next
//! round. In `matmul` `r * n` reaches the head of the `cc` loop that way.

use crate::fir::{BinOp, Func, Inst, Op, Term, Val};
use std::collections::HashSet;

/// Hoists loop invariant instructions into the preheader. Yields the count.
pub(crate) fn hoist_loop_invariants(f: &mut Func) -> usize {
    let n = f.blocks.len();
    if n < 2 {
        return 0;
    }
    // ROUND 82 — the cheap pre-check. Measured on `bin/firnc1.fi`: this pass
    // was 25.5 % of the optimizer, and the optimizer 61 % of the whole
    // compile. Most of that went on functions WITHOUT A LOOP, which still
    // paid for `preds` and the dominator matrix before the back edge search
    // found nothing.
    //
    // If EVERY edge goes strictly forward in the block numbering, the control
    // flow graph is acyclic and there is no natural loop to find. That is a
    // sufficient condition, not a necessary one: a function whose numbering
    // is not in reverse post order may have a backward edge without a loop,
    // and then the full search runs as before. Cheap, sound, and it never
    // changes the result.
    let any_backward = f
        .blocks
        .iter()
        .enumerate()
        .any(|(b, blk)| blk.term.successors().into_iter().any(|s| (s as usize) <= b));
    if !any_backward {
        return 0;
    }
    let preds = crate::mem2reg::preds(f);
    let dom = crate::mem2reg::dominators(f);
    let mut moved = 0;

    // Back edges: b -> h, where h dominates the block b.
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (b, blk) in f.blocks.iter().enumerate() {
        for s in blk.term.successors() {
            let h = s as usize;
            if h < n && dom[b][h] {
                edges.push((h, b));
            }
        }
    }
    if edges.is_empty() {
        return 0;
    }

    // Innermost loops first: smaller body = further inside.
    let mut loops: Vec<(usize, HashSet<usize>)> = Vec::new();
    for (h, b) in edges {
        loops.push((h, natural_loop(h, b, &preds)));
    }
    loops.sort_by_key(|(_, body)| body.len());

    for (head, body) in loops {
        let preheader = match preheader_of(f, head, &body, &preds) {
            Some(p) => p,
            None => continue,
        };
        moved += hoist_out(f, head, &body, preheader);
    }
    moved
}

/// Body of the natural loop belonging to the back edge `back -> head`:
/// `head` plus everything that reaches `back` without passing `head`.
fn natural_loop(head: usize, back: usize, preds: &[Vec<usize>]) -> HashSet<usize> {
    let mut body = HashSet::new();
    body.insert(head);
    let mut stack = Vec::new();
    if back != head {
        body.insert(back);
        stack.push(back);
    }
    while let Some(b) = stack.pop() {
        for &p in &preds[b] {
            if body.insert(p) {
                stack.push(p);
            }
        }
    }
    body
}

/// The one predecessor of the head outside the loop — and only if it jumps
/// there with a plain `br`. Given several entries the loop is skipped:
/// introducing a preheader would shift the block numbers, and that is not
/// worth this pass.
fn preheader_of(f: &Func, head: usize, body: &HashSet<usize>, preds: &[Vec<usize>]) -> Option<usize> {
    let mut outer = preds[head].iter().copied().filter(|p| !body.contains(p));
    let p = outer.next()?;
    if outer.next().is_some() {
        return None;
    }
    match f.blocks[p].term {
        Term::Br(t) if t as usize == head => Some(p),
        _ => None,
    }
}

/// May this instruction be moved at all? (purity + trap freedom)
fn hoistable_op(op: &Op) -> bool {
    if crate::mem2reg::is_untouchable(op) {
        return false;
    }
    match op {
        // Division and remainder can raise a CPU exception — never execute
        // unconditionally just because the value is invariant.
        Op::Bin(BinOp::Div, _, _) | Op::Bin(BinOp::Rem, _, _) => false,
        Op::Const(_)
        | Op::Bin(..)
        | Op::Cmp { .. }
        | Op::Un(..)
        | Op::Cast { .. }
        | Op::PtrAdd { .. } => true,
        _ => false,
    }
}

// ROUND 87 -- THE SAME QUADRATIC SHAPE AS IN `merge_blocks`.
//
// This function used to rebuild, FOR EVERY SINGLE HOISTED INSTRUCTION, the
// set of all values defined inside the loop (a hash set over every
// instruction of the body) and to sort and CLONE the block list of the body
// twice per search step. A loop from which twenty instructions move
// therefore walked its own body twenty times and allocated forty vectors.
//
// The set is now built once and kept up to date: an instruction that moves
// into the preheader defines its value OUTSIDE the loop from then on, so its
// `dst` leaves the set -- which is exactly what makes the instructions that
// depended on it movable in the next step. The block order of the body is
// sorted once.
//
// The search still starts at the beginning after every hoist, so the ORDER
// in which instructions move is unchanged and the result is the same
// instruction sequence as before. Measured over bin/firnc1.fi, the assembler
// is octet-identical.
fn hoist_out(f: &mut Func, head: usize, body: &HashSet<usize>, preheader: usize) -> usize {
    let mut moved = 0;
    let mut buf: Vec<Val> = Vec::new();
    let mut order: Vec<usize> = body.iter().copied().collect();
    order.sort_unstable();
    let mut in_loop: HashSet<Val> = HashSet::new();
    for &b in &order {
        for i in &f.blocks[b].insts {
            if let Some(d) = i.dst {
                in_loop.insert(d);
            }
        }
    }
    loop {
        // Look for the first movable instruction (by block order).
        let mut hit: Option<(usize, usize)> = None;
        'search: for &b in &order {
            for (ix, i) in f.blocks[b].insts.iter().enumerate() {
                if !hoistable_op(&i.op) {
                    continue;
                }
                let d = match i.dst {
                    Some(d) => d,
                    None => continue,
                };
                if f.is_secret(d) {
                    continue;
                }
                buf.clear();
                i.op.uses(&mut buf);
                if buf.iter().any(|v| in_loop.contains(v) || f.is_secret(*v)) {
                    continue;
                }
                // The head itself may keep its condition: an instruction that
                // the terminator of the head needs is hoistable indeed, but the
                // gain is zero. We hoist it anyway -- it is invariant, so the
                // condition is invariant too.
                let _ = head;
                hit = Some((b, ix));
                break 'search;
            }
        }
        let (b, ix) = match hit {
            Some(x) => x,
            None => break,
        };
        // Move: to the end of the preheader, in front of its terminator.
        let inst: Inst = f.blocks[b].insts.remove(ix);
        if let Some(d) = inst.dst {
            in_loop.remove(&d);
        }
        f.blocks[preheader].insts.push(inst);
        moved += 1;
        if moved > 10_000 {
            break; // hard brake, cannot happen
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{BinOp, Block, FTy, Func, Inst, Op, Term};

    /// `bb0: br bb1` · `bb1: cmp/brcond` · `bb2: %x = mul p0,p1 ; br bb1`
    fn loop_with_invariant_multiplication() -> Func {
        let mut f = Func::new("t", vec![FTy::U64, FTy::U64], FTy::U64);
        // %0, %1 are parameters
        let c = f.new_val_pub(FTy::U64);
        let m = f.new_val_pub(FTy::U64);
        f.blocks = vec![
            Block { id: 0, insts: vec![], term: Term::Br(1) },
            Block {
                id: 1,
                insts: vec![Inst::new(
                    Some(c),
                    FTy::Bool,
                    Op::Cmp { op: crate::fir::CmpOp::Lt, ty: FTy::U64, a: 0, b: 1 },
                )],
                term: Term::BrCond { cond: c, then_bb: 2, else_bb: 3 },
            },
            Block {
                id: 2,
                insts: vec![Inst::new(Some(m), FTy::U64, Op::Bin(BinOp::Mul, 0, 1))],
                term: Term::Br(1),
            },
            Block { id: 3, insts: vec![], term: Term::Ret(Some(0)) },
        ];
        f
    }

    #[test]
    fn invariant_multiplication_moves_in_the_preheader() {
        let mut f = loop_with_invariant_multiplication();
        let n = hoist_loop_invariants(&mut f);
        assert!(n >= 1, "nothing hoisted");
        assert!(
            f.blocks[2].insts.is_empty(),
            "the multiplication is still in the body: {:?}",
            f.blocks[2].insts
        );
        assert!(
            f.blocks[0].insts.iter().any(|i| matches!(i.op, Op::Bin(BinOp::Mul, 0, 1))),
            "the multiplication did not end up in the preheader"
        );
    }

    #[test]
    fn division_stays_in_the_loop() {
        let mut f = loop_with_invariant_multiplication();
        f.blocks[2].insts[0].op = Op::Bin(BinOp::Div, 0, 1);
        hoist_loop_invariants(&mut f);
        assert_eq!(
            f.blocks[2].insts.len(),
            1,
            "a division must NEVER be executed unconditionally (division by zero)"
        );
    }

    #[test]
    fn dependent_value_stays_inside() {
        let mut f = loop_with_invariant_multiplication();
        // %m depends on a load -> not invariant
        let l = f.new_val_pub(FTy::U64);
        f.blocks[2].insts.insert(
            0,
            Inst::new(Some(l), FTy::U64, Op::Load { addr: 0 }),
        );
        f.blocks[2].insts[1].op = Op::Bin(BinOp::Mul, l, 1);
        hoist_loop_invariants(&mut f);
        assert_eq!(f.blocks[2].insts.len(), 2, "nothing should have moved");
    }
}
