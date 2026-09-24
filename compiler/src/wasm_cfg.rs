// SPDX-License-Identifier: MPL-2.0
//! **ROUND WASM — from a control flow GRAPH to control flow STRUCTURE.**
//!
//! FIR says "jump to bb7". WebAssembly has no jump: it has `block`, `loop`
//! and `if`, and `br n` leaves the n-th enclosing one of them. Every Firn
//! function therefore has to be rewritten from a graph of basic blocks
//! into properly nested constructs before a single WebAssembly instruction
//! can be written. This file computes what that rewriting needs to know;
//! `codegen_wasm.rs` walks it.
//!
//! ## The method: dominator tree, after Ramsey (ICFP 2022)
//!
//! Norman Ramsey, *Beyond Relooper: Recursive Translation of Unstructured
//! Control Flow to Structured Control Flow*. The idea, in three sentences:
//!
//!   * Walk the DOMINATOR tree, not the graph. A block that only one other
//!     block can reach is written INSIDE that block's code, where the jump
//!     to it stood (it needs no label at all).
//!   * A block that is reached along two or more forward edges — a MERGE
//!     node — is written right AFTER a `block` that encloses everything that
//!     jumps to it; a jump to it is `br` out of that `block`.
//!   * A block that is the target of a backward edge is a LOOP HEADER; its
//!     code is wrapped in a `loop`, and a jump back to it is `br` to that
//!     `loop`.
//!
//! That translation is exact for every REDUCIBLE graph — every loop has one
//! entry — and it needs neither a helper variable nor duplicated code. The
//! analyses it rests on are the textbook ones: a reverse postorder, the
//! dominator algorithm of Cooper, Harvey and Kennedy, and the classification
//! of retreating edges.
//!
//! ## And when the graph is not reducible
//!
//! Firn source has only structured loops, so the lowering never builds an
//! irreducible graph. The OPTIMIZER may: jump threading (`threading.rs`)
//! redirects edges, and redirecting an edge into the middle of a loop gives
//! the loop a second entry. For that case `reducible` comes back `false` and
//! `codegen_wasm.rs` falls back to the one translation that works for ANY
//! graph: a `loop` around a `br_table` that dispatches on a block number.
//! It is slower, and it is correct; the fallback is counted in
//! `--stats` so that nobody has to guess how often it happens.

use crate::fir::{BlockId, Func, Term};

pub const NONE: usize = usize::MAX;

pub struct Cfg {
    /// the reachable blocks, in reverse postorder (the entry first)
    pub order: Vec<BlockId>,
    /// the position in `order` per block id; `NONE` = unreachable
    pub rpo: Vec<usize>,
    /// dominator tree children per block id, ascending by `rpo`
    pub children: Vec<Vec<BlockId>>,
    /// target of at least one backward edge
    pub loop_header: Vec<bool>,
    /// reached along two or more forward edges (counted WITH multiplicity,
    /// so `brcond %c, bb3, bb3` makes bb3 one), or the target of a `switch`
    pub merge: Vec<bool>,
    /// false = a retreating edge whose target does not dominate its source
    pub reducible: bool,
}

/// The successors of a terminator, WITH repetitions (a `switch` with three
/// cases into the same block names it three times).
pub fn succs(t: &Term) -> Vec<BlockId> {
    t.successors()
}

pub fn analyse(f: &Func) -> Cfg {
    let n = f.blocks.len();
    let succ: Vec<Vec<BlockId>> = f.blocks.iter().map(|b| succs(&b.term)).collect();

    // ---- reverse postorder, iteratively (a deep graph must not be able to
    // overflow the compiler's own stack)
    let mut visited = vec![false; n];
    let mut post: Vec<BlockId> = Vec::with_capacity(n);
    if n > 0 {
        let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
        visited[0] = true;
        while let Some(top) = stack.last_mut() {
            let (b, i) = *top;
            if i < succ[b].len() {
                top.1 += 1;
                let s = succ[b][i] as usize;
                if s < n && !visited[s] {
                    visited[s] = true;
                    stack.push((s, 0));
                }
            } else {
                post.push(b as BlockId);
                stack.pop();
            }
        }
    }
    let order: Vec<BlockId> = post.iter().rev().copied().collect();
    let mut rpo = vec![NONE; n];
    for (i, b) in order.iter().enumerate() {
        rpo[*b as usize] = i;
    }

    // ---- predecessors among the reachable blocks
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &b in &order {
        for &s in &succ[b as usize] {
            let s = s as usize;
            if s < n && !preds[s].contains(&(b as usize)) {
                preds[s].push(b as usize);
            }
        }
    }

    // ---- dominators (Cooper, Harvey, Kennedy: "A Simple, Fast Dominance
    // Algorithm"), iterated over the reverse postorder until nothing moves
    let mut idom = vec![NONE; n];
    if n > 0 {
        idom[0] = 0;
    }
    let intersect = |idom: &Vec<usize>, mut a: usize, mut b: usize| -> usize {
        while a != b {
            while rpo[a] > rpo[b] {
                a = idom[a];
            }
            while rpo[b] > rpo[a] {
                b = idom[b];
            }
        }
        a
    };
    let mut changed = true;
    while changed {
        changed = false;
        for &b in order.iter().skip(1) {
            let b = b as usize;
            let mut new = NONE;
            for &p in &preds[b] {
                if idom[p] == NONE {
                    continue;
                }
                new = if new == NONE { p } else { intersect(&idom, p, new) };
            }
            if new != NONE && idom[b] != new {
                idom[b] = new;
                changed = true;
            }
        }
    }
    let dominates = |a: usize, mut b: usize| -> bool {
        loop {
            if a == b {
                return true;
            }
            if b == 0 || idom[b] == NONE || idom[b] == b {
                return false;
            }
            b = idom[b];
        }
    };

    // ---- edges: backward (loop), forward (counts toward merge), switch
    let mut loop_header = vec![false; n];
    let mut forward_in = vec![0u32; n];
    let mut merge = vec![false; n];
    let mut reducible = true;
    for &b in &order {
        let bu = b as usize;
        let is_switch = matches!(f.blocks[bu].term, Term::Switch { .. });
        for &s in &succ[bu] {
            let su = s as usize;
            if su >= n {
                continue;
            }
            if rpo[su] <= rpo[bu] {
                if dominates(su, bu) {
                    loop_header[su] = true;
                } else {
                    reducible = false;
                }
            } else {
                forward_in[su] += 1;
                // A `br_table` can only name labels; so every forward
                // target of a switch has to BE one, even the ones only this
                // switch reaches.
                if is_switch {
                    merge[su] = true;
                }
            }
        }
    }
    for b in 0..n {
        if forward_in[b] >= 2 {
            merge[b] = true;
        }
    }

    // ---- the dominator tree
    let mut children: Vec<Vec<BlockId>> = vec![Vec::new(); n];
    for &b in order.iter().skip(1) {
        let d = idom[b as usize];
        if d != NONE {
            children[d].push(b);
        }
    }
    // `order` is already the reverse postorder, so every child list came
    // out ascending by `rpo` on its own.
    Cfg { order, rpo, children, loop_header, merge, reducible }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{FTy, Func, Op, Term};

    /// bb0 -> bb1 (loop header) -> bb2 -> bb1, bb1 -> bb3
    #[test]
    fn a_while_loop_has_one_header_and_is_reducible() {
        let mut f = Func::new("f", vec![], FTy::Void);
        let b1 = f.add_block();
        let b2 = f.add_block();
        let b3 = f.add_block();
        let c = f.push(0, FTy::Bool, Op::Const(1));
        f.set_term(0, Term::Br(b1));
        f.set_term(b1, Term::BrCond { cond: c, then_bb: b2, else_bb: b3 });
        f.set_term(b2, Term::Br(b1));
        f.set_term(b3, Term::Ret(None));
        let g = analyse(&f);
        assert!(g.reducible);
        assert!(g.loop_header[b1 as usize]);
        assert!(!g.loop_header[b2 as usize]);
        assert!(!g.merge[b3 as usize]);
        // bb3 is dominated by the loop header, not by the loop body
        assert!(g.children[b1 as usize].contains(&b3));
    }

    /// Two entries into the same cycle: bb0 -> bb1, bb0 -> bb2, bb1 <-> bb2.
    #[test]
    fn a_loop_with_two_entries_is_irreducible() {
        let mut f = Func::new("f", vec![], FTy::Void);
        let b1 = f.add_block();
        let b2 = f.add_block();
        let c = f.push(0, FTy::Bool, Op::Const(1));
        f.set_term(0, Term::BrCond { cond: c, then_bb: b1, else_bb: b2 });
        f.set_term(b1, Term::Br(b2));
        f.set_term(b2, Term::BrCond { cond: c, then_bb: b1, else_bb: b1 });
        let g = analyse(&f);
        assert!(!g.reducible);
    }

    /// `brcond %c, bb1, bb1` reaches bb1 twice: it is a merge node.
    #[test]
    fn two_edges_into_the_same_block_make_it_a_merge() {
        let mut f = Func::new("f", vec![], FTy::Void);
        let b1 = f.add_block();
        let c = f.push(0, FTy::Bool, Op::Const(1));
        f.set_term(0, Term::BrCond { cond: c, then_bb: b1, else_bb: b1 });
        f.set_term(b1, Term::Ret(None));
        let g = analyse(&f);
        assert!(g.merge[b1 as usize]);
    }
}
