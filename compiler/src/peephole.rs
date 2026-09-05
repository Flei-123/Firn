// SPDX-License-Identifier: GPL-2.0-only
//! **ROUND 82** — the three things the optimizer was leaving on the table.
//!
//! Found the way the round asked for it: small Firn programs, `objdump -d`
//! next to what `gcc -O2` makes of the same C. Three differences were real,
//! everything else the existing passes already had (`lea` for address
//! arithmetic, `imul` by a power of two as `shl`, common subexpressions, the
//! fusion of `cmp` and `jcc` for a comparison that a `brcond` reads directly).
//!
//! **1. A NEGATED COMPARISON.** `if !(a < b)` came out as
//!
//! ```text
//!     cmp r8, r9 ; setl al ; movzx r10d, al ; mov rax, r10
//!     xor eax, 1 ; mov r9, rax ; test r9b, r9b ; jnz .L
//! ```
//!
//! eight instructions, where `gcc -O2` writes `cmp rdi, rsi ; jge .L`. The
//! `not` of a comparison IS a comparison — with the opposite operator. Two
//! instructions instead of eight.
//!
//! **FLOATING POINT DOES NOT JOIN IN.** `!(a < b)` and `a >= b` are the same
//! thing for integers and DIFFERENT things for IEEE-754: with a NaN on either
//! side `a < b` is false, so `!(a < b)` is true, while `a >= b` is false as
//! well. Every ordering comparison with NaN is false, and inverting the
//! operator would quietly break that. The guard is `!ty.is_float()`.
//!
//! **2. `brcond` OVER A NEGATION.** `if !flag` for a `flag` that is not a
//! comparison stays `xor 1 ; test ; jnz`. The negation is unnecessary: a
//! branch that swaps its two targets does the same. That also catches the
//! case where the negated thing is a call result or a loaded octet.
//!
//! **3. UNSIGNED DIVISION AND REMAINDER BY A POWER OF TWO.** `a / 8` on a
//! `u64` produced
//!
//! ```text
//!     mov r9, 8 ; mov rax, r8 ; mov rcx, r9 ; xor edx, edx ; div rcx
//! ```
//!
//! with a `div` in it, and `div r64` costs some 20 to 40 cycles on this
//! processor against one for `shr`. `a % 8` was the same instruction with
//! `rdx` read instead of `rax`. `gcc -O2` writes `shr rax, 3` and
//! `and eax, 7`.
//!
//! **ONLY UNSIGNED.** For a signed type the two are NOT the same: C and Firn
//! round towards zero, an arithmetic right shift rounds towards minus
//! infinity, so `-1 / 2` is `0` and `-1 >> 1` is `-1`. The correct signed
//! sequence needs a bias (`sar`/`add`/`sar`) and three more instructions;
//! it is not in this round and is named as open in `docs/ROUND82.md` §7.

use crate::fir::{BinOp, CmpOp, FTy, Func, Inst, Op, Term, UnOp, Val};
use std::collections::HashMap;

/// Which comparison is the negation of this one?
fn invert(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Eq => CmpOp::Ne,
        CmpOp::Ne => CmpOp::Eq,
        CmpOp::Lt => CmpOp::Ge,
        CmpOp::Ge => CmpOp::Lt,
        CmpOp::Le => CmpOp::Gt,
        CmpOp::Gt => CmpOp::Le,
    }
}

/// `Some(k)` when `v == 1 << k` and `k >= 1`.
fn log2_exact(v: i128) -> Option<u32> {
    if v <= 1 {
        return None;
    }
    if v & (v - 1) != 0 {
        return None;
    }
    Some(v.trailing_zeros())
}

/// Runs all three transformations; the return value is how many places
/// changed (for `OptStats`).
pub(crate) fn run(f: &mut Func) -> usize {
    let mut changed = 0usize;

    // ---- the tables this pass reads --------------------------------------
    // constants, comparisons and negations, each by their result value
    let mut consts: HashMap<Val, i128> = HashMap::new();
    let mut cmps: HashMap<Val, (CmpOp, FTy, Val, Val)> = HashMap::new();
    let mut nots: HashMap<Val, Val> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            let d = match i.dst {
                Some(d) => d,
                None => continue,
            };
            match &i.op {
                Op::Const(c) => {
                    consts.insert(d, *c);
                }
                Op::Cmp { op, ty, a, b: b2 } => {
                    cmps.insert(d, (*op, *ty, *a, *b2));
                }
                Op::Un(UnOp::Not, x) if i.ty == FTy::Bool => {
                    nots.insert(d, *x);
                }
                _ => {}
            }
        }
    }

    // ---- 1. `not` of a comparison -> the inverted comparison --------------
    for b in f.blocks.iter_mut() {
        for i in b.insts.iter_mut() {
            let src = match (&i.op, i.ty) {
                (Op::Un(UnOp::Not, x), FTy::Bool) => *x,
                _ => continue,
            };
            if let Some((op, ty, a, b2)) = cmps.get(&src).copied() {
                // IEEE-754: with a NaN every ordering comparison is false, so
                // `!(a < b)` and `a >= b` are different things. Integers only.
                if ty.is_float() {
                    continue;
                }
                i.op = Op::Cmp { op: invert(op), ty, a, b: b2 };
                changed += 1;
            }
        }
    }

    // ---- 2. `brcond` over a negation -> swap the two targets --------------
    //
    // Repeated until nothing moves: `!!x` is two negations, and the second
    // pass through takes the second one.
    let mut again = true;
    while again {
        again = false;
        for b in f.blocks.iter_mut() {
            if let Term::BrCond { cond, then_bb, else_bb } = b.term {
                if let Some(x) = nots.get(&cond).copied() {
                    b.term = Term::BrCond { cond: x, then_bb: else_bb, else_bb: then_bb };
                    changed += 1;
                    again = true;
                }
            }
        }
    }

    // ---- 3. unsigned `/` and `%` by a power of two ------------------------
    //
    // A new instruction is needed for the shift amount resp. the mask, and it
    // has to stand IN FRONT of the place that reads it. Collected first, then
    // inserted -- rewriting a `Vec` while iterating over it is how one loses
    // an index.
    struct Edit {
        block: usize,
        at: usize,
        /// `true` = `shr`, `false` = `and`
        shift: bool,
        ty: FTy,
        a: Val,
        dst: Val,
        imm: i128,
    }
    let mut edits: Vec<Edit> = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            let d = match i.dst {
                Some(d) => d,
                None => continue,
            };
            let (op, a, b2) = match &i.op {
                Op::Bin(op @ (BinOp::Div | BinOp::Rem), a, b2) => (*op, *a, *b2),
                _ => continue,
            };
            // Signed rounds towards zero, `sar` towards minus infinity —
            // see the header. Floating point has no `Rem` and its `Div` is
            // not this instruction.
            if i.ty.signed() || i.ty.is_float() {
                continue;
            }
            let k = match consts.get(&b2).copied().and_then(log2_exact) {
                Some(k) => k,
                None => continue,
            };
            if k >= i.ty.bits() {
                continue;
            }
            if op == BinOp::Div {
                edits.push(Edit { block: bi, at: ii, shift: true, ty: i.ty, a, dst: d, imm: k as i128 });
            } else {
                let mask = (1i128 << k) - 1;
                edits.push(Edit { block: bi, at: ii, shift: false, ty: i.ty, a, dst: d, imm: mask });
            }
        }
    }
    if !edits.is_empty() {
        // From the back, so that the indices in front stay right.
        edits.sort_by(|x, y| (y.block, y.at).cmp(&(x.block, x.at)));
        for e in edits {
            let cv = f.new_val_pub(e.ty);
            let here = f.blocks[e.block].insts[e.at].loc;
            let cinst = Inst::like(Some(cv), e.ty, Op::Const(e.imm), here);
            let newop = if e.shift {
                Op::Bin(BinOp::Shr, e.a, cv)
            } else {
                Op::Bin(BinOp::And, e.a, cv)
            };
            let blk = &mut f.blocks[e.block];
            blk.insts[e.at] = Inst::like(Some(e.dst), e.ty, newop, here);
            blk.insts.insert(e.at, cinst);
            changed += 1;
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{Block, Module, Term};

    fn one(f: &mut Func) -> usize {
        run(f)
    }

    #[test]
    fn negated_comparison_becomes_the_opposite_one() {
        let mut f = Func::new("t", vec![FTy::I64, FTy::I64], FTy::Bool);
        let a = f.param_val(0);
        let b = f.param_val(1);
        let c = f.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I64, a, b });
        let n = f.push(0, FTy::Bool, Op::Un(UnOp::Not, c));
        f.set_term(0, Term::Ret(Some(n)));
        assert_eq!(one(&mut f), 1);
        let last = f.blocks[0].insts.last().unwrap();
        assert!(
            matches!(last.op, Op::Cmp { op: CmpOp::Ge, .. }),
            "{:?}",
            last.op
        );
    }

    #[test]
    fn a_float_comparison_stays_as_it_is() {
        // NaN: `!(a < b)` is true, `a >= b` is false. Inverting would be wrong.
        let mut f = Func::new("t", vec![FTy::F64, FTy::F64], FTy::Bool);
        let a = f.param_val(0);
        let b = f.param_val(1);
        let c = f.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::F64, a, b });
        let n = f.push(0, FTy::Bool, Op::Un(UnOp::Not, c));
        f.set_term(0, Term::Ret(Some(n)));
        assert_eq!(one(&mut f), 0);
    }

    #[test]
    fn unsigned_division_by_eight_becomes_a_shift() {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let a = f.param_val(0);
        let e = f.push(0, FTy::U64, Op::Const(8));
        let q = f.push(0, FTy::U64, Op::Bin(BinOp::Div, a, e));
        f.set_term(0, Term::Ret(Some(q)));
        assert_eq!(one(&mut f), 1);
        assert!(f.blocks[0].insts.iter().any(|i| matches!(i.op, Op::Bin(BinOp::Shr, ..))));
        assert!(!f.blocks[0].insts.iter().any(|i| matches!(i.op, Op::Bin(BinOp::Div, ..))));
    }

    #[test]
    fn unsigned_remainder_by_eight_becomes_a_mask() {
        let mut f = Func::new("t", vec![FTy::U32], FTy::U32);
        let a = f.param_val(0);
        let e = f.push(0, FTy::U32, Op::Const(8));
        let q = f.push(0, FTy::U32, Op::Bin(BinOp::Rem, a, e));
        f.set_term(0, Term::Ret(Some(q)));
        assert_eq!(one(&mut f), 1);
        let masked = f.blocks[0]
            .insts
            .iter()
            .any(|i| matches!(i.op, Op::Bin(BinOp::And, ..)));
        assert!(masked);
        assert!(f.blocks[0].insts.iter().any(|i| matches!(i.op, Op::Const(7))));
    }

    #[test]
    fn signed_division_stays_a_division() {
        // -1 / 2 == 0, but -1 >> 1 == -1.
        let mut f = Func::new("t", vec![FTy::I64], FTy::I64);
        let a = f.param_val(0);
        let e = f.push(0, FTy::I64, Op::Const(2));
        let q = f.push(0, FTy::I64, Op::Bin(BinOp::Div, a, e));
        f.set_term(0, Term::Ret(Some(q)));
        assert_eq!(one(&mut f), 0);
    }

    #[test]
    fn brcond_over_a_negation_swaps_the_targets() {
        let mut f = Func::new("t", vec![FTy::Bool], FTy::I32);
        let a = f.param_val(0);
        let n = f.push(0, FTy::Bool, Op::Un(UnOp::Not, a));
        let b1 = f.add_block();
        let b2 = f.add_block();
        f.set_term(0, Term::BrCond { cond: n, then_bb: b1, else_bb: b2 });
        let z = f.push(b1, FTy::I32, Op::Const(1));
        f.set_term(b1, Term::Ret(Some(z)));
        let o = f.push(b2, FTy::I32, Op::Const(0));
        f.set_term(b2, Term::Ret(Some(o)));
        assert_eq!(one(&mut f), 1);
        match f.blocks[0].term {
            Term::BrCond { cond, then_bb, else_bb } => {
                assert_eq!(cond, a);
                assert_eq!(then_bb, b2);
                assert_eq!(else_bb, b1);
            }
            ref t => panic!("{:?}", t),
        }
    }

    #[test]
    fn the_module_still_builds_after_the_pass() {
        let mut m = Module::new();
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::U64, Op::Const(100));
        let e = f.push(0, FTy::U64, Op::Const(16));
        let q = f.push(0, FTy::U64, Op::Bin(BinOp::Rem, a, e));
        let c = f.push(0, FTy::I32, Op::Cast { src: q, from: FTy::U64 });
        f.set_term(0, Term::Ret(Some(c)));
        run(&mut f);
        m.funcs.push(f);
        let _: &Block = &m.funcs[0].blocks[0];
        assert!(crate::codegen_x86::emit(&m).is_ok());
    }
}
