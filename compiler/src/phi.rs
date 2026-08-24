//! **ROUND 92 — phi elimination.** The last thing that happens to FIR before
//! a code generator sees it.
//!
//! ## Why this file exists, and why it is ONE file
//!
//! `mem2reg.rs` puts phi nodes into FIR because that is the only way to say
//! "the value here depends on which edge we came in on". No machine has an
//! instruction for that. So somewhere the phi has to become what it has
//! always meant on a real processor: **a copy at the end of every
//! predecessor**.
//!
//! Round 90's finding was that one question must not have two answers. There
//! the question was "which registers does this instruction destroy" and the
//! two answers had silently drifted apart for three rounds. Phi elimination
//! is exactly that kind of question — every backend could do it for itself,
//! and there are three of them (the x86 base path, the register aware path
//! in `regalloc.rs`, the A64 path). So it is done **once, here**, on FIR,
//! and every backend afterwards reads a phi-free instruction list exactly as
//! it did before round 92. `Op::Phi` never reaches a backend; the arms that
//! catch it there return an internal error rather than emitting anything.
//!
//! ## The one hard part: the copies of an edge happen SIMULTANEOUSLY
//!
//! A block can carry several phis, and the block they come from gets the
//! copies for all of them **at the same moment**. All reads happen before
//! all writes. Written out one after another that is wrong the moment one
//! phi reads what another one writes:
//!
//! ```text
//! bb3:  %a = phi [bb2 %b, ...]        two values swapping places on the
//!       %b = phi [bb2 %a, ...]        back edge of a loop
//! ```
//!
//! Emitting `%a = copy %b` and then `%b = copy %a` gives both values the
//! same content. That is the **swap problem**, and it is not a corner case —
//! it is what a rotation of two variables in a loop looks like after SSA
//! construction.
//!
//! The answer is the standard sequentialization:
//!
//!  1. drop the copies that copy a value onto itself,
//!  2. emit any copy whose **target is nobody else's source** — after that
//!     one nothing can read the old content any more, so it is safe,
//!  3. if nothing is left that is safe, the rest is a **cycle**: rescue one
//!     target into a fresh value, point every source that named it at the
//!     rescue, and the cycle is broken open at that one place.
//!
//! That is provably correct for any permutation and costs one extra value
//! per cycle, which is one `mov` per cycle in the emitted code.
//!
//! ## Critical edges are deliberately NOT split
//!
//! The textbook says: split an edge from a block with several successors
//! into a block with several predecessors, then put the copies in the new
//! block. That matters when the copies are **coalesced** with their source,
//! because then a copy placed too early can overwrite a value another path
//! still needs (the "lost copy" problem).
//!
//! Nothing here coalesces. A copy defines a **new** value that no other path
//! reads, so writing it at the end of a predecessor that also branches
//! somewhere else writes a value that is dead on that other path — and
//! `regalloc.rs` computes its liveness from the code it actually gets, so it
//! sees exactly that. Splitting would add a block per critical edge for no
//! gain, and every added block lengthens the linear numbering the interval
//! allocator works on. When a later round teaches the allocator to coalesce
//! copies, splitting has to come with it; that is written down in
//! docs/ROUND92.md and not left to be rediscovered.

use crate::fir::{BlockId, Func, Inst, Module, Op, Term, Val};

/// Runs over the whole module. Afterwards no `Op::Phi` exists any more.
pub fn eliminate(m: &mut Module) -> Result<(), String> {
    for f in m.funcs.iter_mut() {
        eliminate_func(f)?;
    }
    Ok(())
}

/// One parallel copy: `dst` gets `src`, all of them at the same moment.
struct Par {
    dst: Val,
    ty: crate::fir::FTy,
    src: Val,
}

pub(crate) fn eliminate_func(f: &mut Func) -> Result<(), String> {
    if !f.has_phi() {
        return Ok(());
    }
    // The entry block cannot carry a phi: the edge the function is entered
    // through has no block to put a copy in. `mem2reg.rs` refuses to promote
    // anything in a function whose entry block has a predecessor, so this is
    // an assertion and not a case.
    if f.blocks.first().map(|b| b.has_phi()).unwrap_or(false) {
        return Err(format!("internal error: @{} has a phi in the entry block", f.name));
    }
    // Trim entries that stopped being edges; that also folds the phis that
    // have only one answer left, so fewer copies get emitted below.
    crate::mem2reg::simplify_phis(f);
    if !f.has_phi() {
        return Ok(());
    }

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

    // What has to be appended to which block. Collected first and applied
    // afterwards, so that the walk reads a function nobody is changing.
    let mut plan: Vec<(usize, Vec<Par>)> = Vec::new();
    for bi in 0..nb {
        let np = f.blocks[bi].phi_count();
        if np == 0 {
            continue;
        }
        for &p in &preds[bi] {
            let mut par: Vec<Par> = Vec::new();
            for i in f.blocks[bi].insts[..np].iter() {
                let d = match i.dst {
                    Some(d) => d,
                    None => continue,
                };
                let inc = match &i.op {
                    Op::Phi { incoming } => incoming,
                    _ => continue,
                };
                match inc.iter().find(|(q, _)| *q as usize == p) {
                    Some((_, v)) => par.push(Par { dst: d, ty: i.ty, src: *v }),
                    None => {
                        return Err(format!(
                            "internal error: @{} bb{}: the phi %{} has no entry for its \
                             predecessor bb{}",
                            f.name, bi, d, p
                        ))
                    }
                }
            }
            if !par.is_empty() {
                plan.push((p, par));
            }
        }
    }

    for (p, par) in plan {
        let seq = sequentialize(f, par);
        f.blocks[p].insts.extend(seq);
    }
    // and the phis themselves go
    for b in f.blocks.iter_mut() {
        b.insts.retain(|i| !matches!(i.op, Op::Phi { .. }));
    }
    Ok(())
}

/// Turns ONE parallel copy into a sequence of `Op::Copy` instructions that
/// has the same effect. See the header for why this is not simply a loop.
fn sequentialize(f: &mut Func, mut par: Vec<Par>) -> Vec<Inst> {
    let mut out: Vec<Inst> = Vec::new();
    // 1. `x = x` does nothing.
    par.retain(|c| c.dst != c.src);
    let mut guard = 0usize;
    while !par.is_empty() {
        guard += 1;
        if guard > 4 * par.len() + 64 {
            break; // cannot happen: every round either emits or breaks a cycle
        }
        // 2. a target nobody else still reads
        let free = par.iter().position(|c| !par.iter().any(|o| o.src == c.dst));
        match free {
            Some(k) => {
                let c = par.remove(k);
                out.push(Inst { dst: Some(c.dst), ty: c.ty, op: Op::Copy { src: c.src } });
            }
            None => {
                // 3. everything left is a cycle: rescue one target
                let ty = par[0].ty;
                let victim = par[0].dst;
                let tmp = f.new_val_pub(ty);
                out.push(Inst { dst: Some(tmp), ty, op: Op::Copy { src: victim } });
                for c in par.iter_mut() {
                    if c.src == victim {
                        c.src = tmp;
                    }
                }
            }
        }
    }
    out
}

/// The block numbers a terminator names, as mutable references — used by the
/// tests below and by anybody who needs to rewrite edges.
#[allow(dead_code)]
pub(crate) fn term_targets(t: &mut Term) -> Vec<&mut BlockId> {
    match t {
        Term::Br(b) => vec![b],
        Term::BrCond { then_bb, else_bb, .. } => vec![then_bb, else_bb],
        Term::Switch { cases, default, .. } => {
            let mut v: Vec<&mut BlockId> = cases.iter_mut().map(|(_, b)| b).collect();
            v.push(default);
            v
        }
        Term::Ret(_) | Term::Unset => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{BinOp, CmpOp, FTy};

    /// The swap: two phis on one edge that read each other. Emitted naively
    /// both values end up the same; this is the test that says they do not.
    #[test]
    fn swapping_phis_do_not_collapse() {
        let mut f = Func::new("swap", vec![FTy::I64, FTy::I64], FTy::I64);
        let head = f.add_block();
        let body = f.add_block();
        let done = f.add_block();
        f.set_term(0, Term::Br(head));
        // bb1: %a = phi [bb0 %0, bb2 %b] ; %b = phi [bb0 %1, bb2 %a]
        let a = f.new_val_pub(FTy::I64);
        let b = f.new_val_pub(FTy::I64);
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(a),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 0), (body, b)] },
        });
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(b),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 1), (body, a)] },
        });
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I64, a, b });
        f.set_term(head, Term::BrCond { cond: c, then_bb: body, else_bb: done });
        f.set_term(body, Term::Br(head));
        f.set_term(done, Term::Ret(Some(a)));
        assert!(f.verify_phis().is_ok(), "{:?}", f.verify_phis());

        eliminate_func(&mut f).unwrap();
        assert!(!f.has_phi());
        // The back edge block now holds three copies, not two: one of them
        // is the rescue that breaks the cycle open.
        let copies: Vec<(Val, Val)> = f.blocks[body as usize]
            .insts
            .iter()
            .filter_map(|i| match (&i.op, i.dst) {
                (Op::Copy { src }, Some(d)) => Some((d, *src)),
                _ => None,
            })
            .collect();
        assert_eq!(copies.len(), 3, "a two-cycle needs one rescue: {:?}", copies);
        // Play the sequence through: a and b really do swap.
        let mut env: std::collections::HashMap<Val, i64> =
            std::collections::HashMap::new();
        env.insert(a, 11);
        env.insert(b, 22);
        for (d, s) in &copies {
            let v = *env.get(s).unwrap_or(&0);
            env.insert(*d, v);
        }
        assert_eq!(env[&a], 22);
        assert_eq!(env[&b], 11);
    }

    /// A chain (not a cycle) needs no rescue, and the order matters: the
    /// target that nobody reads any more has to be written first.
    #[test]
    fn chained_copies_keep_their_order() {
        let mut f = Func::new("chain", vec![FTy::I64, FTy::I64], FTy::I64);
        let head = f.add_block();
        let body = f.add_block();
        let done = f.add_block();
        f.set_term(0, Term::Br(head));
        let x = f.new_val_pub(FTy::I64);
        let y = f.new_val_pub(FTy::I64);
        // %x = phi [bb0 %0, bb2 %y] ; %y = phi [bb0 %1, bb2 %0]
        // On the back edge that is the parallel copy { x <- y, y <- %0 }.
        // `x` is nobody's source, so it has to be written FIRST; the other
        // order would give x the new y.
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(x),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 0), (body, y)] },
        });
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(y),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 1), (body, 0)] },
        });
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I64, a: x, b: y });
        f.set_term(head, Term::BrCond { cond: c, then_bb: body, else_bb: done });
        f.set_term(body, Term::Br(head));
        f.set_term(done, Term::Ret(Some(x)));
        eliminate_func(&mut f).unwrap();
        let copies: Vec<(Val, Val)> = f.blocks[body as usize]
            .insts
            .iter()
            .filter_map(|i| match (&i.op, i.dst) {
                (Op::Copy { src }, Some(d)) => Some((d, *src)),
                _ => None,
            })
            .collect();
        assert_eq!(copies.len(), 2);
        assert_eq!(copies[0], (x, y));
        assert_eq!(copies[1], (y, 0));
    }

    /// A loop counter, end to end: `mem2reg` promotes it, `phi.rs` takes the
    /// phi apart again, and nothing is left standing in memory.
    #[test]
    fn a_loop_counter_ends_up_without_an_alloca() {
        // fn f(n: i64) -> i64 { var i = 0; while i < n { i = i + 1 } return i }
        let mut f = Func::new("count", vec![FTy::I64], FTy::I64);
        let slot = f.alloca(8, 8);
        let z = f.push(0, FTy::I64, Op::Const(0));
        f.push_void(0, FTy::I64, Op::Store { addr: slot, val: z });
        let head = f.add_block();
        let body = f.add_block();
        let done = f.add_block();
        f.set_term(0, Term::Br(head));
        let l1 = f.push(head, FTy::I64, Op::Load { addr: slot });
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I64, a: l1, b: 0 });
        f.set_term(head, Term::BrCond { cond: c, then_bb: body, else_bb: done });
        let l2 = f.push(body, FTy::I64, Op::Load { addr: slot });
        let one = f.push(body, FTy::I64, Op::Const(1));
        let s = f.push(body, FTy::I64, Op::Bin(BinOp::Add, l2, one));
        f.push_void(body, FTy::I64, Op::Store { addr: slot, val: s });
        f.set_term(body, Term::Br(head));
        let l3 = f.push(done, FTy::I64, Op::Load { addr: slot });
        f.set_term(done, Term::Ret(Some(l3)));

        assert!(crate::mem2reg::promote_allocas(&mut f) >= 3);
        assert!(f.verify_phis().is_ok(), "{:?}", f.verify_phis());
        // No alloca, no load, no store left anywhere.
        for b in &f.blocks {
            for i in &b.insts {
                assert!(
                    !matches!(i.op, Op::Alloca { .. } | Op::Load { .. } | Op::Store { .. }),
                    "{} still touches memory",
                    crate::fir::Module { funcs: vec![f.clone()] }.to_text()
                );
            }
        }
        assert!(f.has_phi());
        eliminate_func(&mut f).unwrap();
        assert!(!f.has_phi());
    }
}
