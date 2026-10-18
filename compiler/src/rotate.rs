// SPDX-License-Identifier: MPL-2.0
//! **r837 — 32-bit rotate recognition (aarch64 only).**
//!
//! SHA-256 style code written with 64-bit values that carry 32-bit data
//! rotates like this (`bench_android/bcpu.fi`, `rotr32`):
//!
//! ```text
//!     (((x & 0xFFFFFFFF) >> n) | (x << (32 - n))) & 0xFFFFFFFF
//! ```
//!
//! Five operations, each one a separate value for the allocator. AArch64 has
//! `ror w, w, #n`: it reads the low 32 bits, rotates and zero-extends. That is
//! EXACTLY what the expression computes for `1 <= n <= 31`, so the whole
//! expression becomes one `BinOp::Rotr32`. The 64-bit input needs no mask
//! beforehand: the instruction only ever looks at the low half.
//!
//! What is matched (all operations `u64`, constants checked by value):
//!
//! ```text
//!     r = and( or( shr(A, n), shl(B, m) ), 0xFFFFFFFF )      n + m == 32
//!     A is `and(B, 0xFFFFFFFF)`, or A is B and B is already below 2^32
//! ```
//!
//! The pass runs once per function, after the fixpoint and after inlining
//! (so `rotr32` is already a part of the caller and its shift counts are
//! constants), and only for `--target=aarch64-linux`: no other backend
//! implements `Rotr32`. The shifts that fed the old expression become dead
//! and `dce` removes them.

use crate::fir::{BinOp, FTy, Func, Op, Val};
use std::collections::HashMap;

const M32: i128 = 0xFFFF_FFFF;

/// How many expressions were turned into a `Rotr32`.
pub(crate) fn run(f: &mut Func) -> usize {
    // ---- tables (SSA: one definition per value) ---------------------------
    let mut consts: HashMap<Val, i128> = HashMap::new();
    // u64 binary operations by result value
    let mut bins: HashMap<Val, (BinOp, Val, Val)> = HashMap::new();
    // values that are known to be below 2^32 for another reason: a zero
    // extended narrow load/cast
    let mut small: HashMap<Val, ()> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            let d = match i.dst {
                Some(d) => d,
                None => continue,
            };
            match &i.op {
                Op::Const(c) if i.ty == FTy::U64 => {
                    consts.insert(d, FTy::U64.truncate(*c));
                }
                Op::Bin(op, a, b2) if i.ty == FTy::U64 => {
                    bins.insert(d, (*op, *a, *b2));
                }
                Op::Cast { from, .. }
                    if i.ty == FTy::U64 && matches!(from, FTy::U8 | FTy::U16 | FTy::U32 | FTy::Bool) =>
                {
                    small.insert(d, ());
                }
                _ => {}
            }
        }
    }

    // is the value known to be below 2^32?
    let clean = |v: Val, bins: &HashMap<Val, (BinOp, Val, Val)>, consts: &HashMap<Val, i128>, small: &HashMap<Val, ()>| -> bool {
        if small.contains_key(&v) {
            return true;
        }
        match bins.get(&v) {
            Some((BinOp::Rotr32, ..)) => true,
            Some((BinOp::And, x, y)) => {
                consts.get(x).map_or(false, |c| *c <= M32) || consts.get(y).map_or(false, |c| *c <= M32)
            }
            _ => false,
        }
    };

    let mut changed = 0usize;
    for bi in 0..f.blocks.len() {
        for ii in 0..f.blocks[bi].insts.len() {
            let (d, x, y) = {
                let i = &f.blocks[bi].insts[ii];
                match (&i.op, i.dst) {
                    (Op::Bin(BinOp::And, x, y), Some(d)) if i.ty == FTy::U64 => (d, *x, *y),
                    _ => continue,
                }
            };
            // which operand is the 0xFFFFFFFF mask?
            let or_val = if consts.get(&y) == Some(&M32) {
                x
            } else if consts.get(&x) == Some(&M32) {
                y
            } else {
                continue;
            };
            let (p, q) = match bins.get(&or_val) {
                Some((BinOp::Or, p, q)) => (*p, *q),
                _ => continue,
            };
            let mut hit: Option<(Val, Val)> = None; // (rotated value, shift count value)
            for (sr, sl) in [(p, q), (q, p)] {
                let (a_src, n_val) = match bins.get(&sr) {
                    Some((BinOp::Shr, a, n)) => (*a, *n),
                    _ => continue,
                };
                let (b_src, m_val) = match bins.get(&sl) {
                    Some((BinOp::Shl, b, m)) => (*b, *m),
                    _ => continue,
                };
                let (n, m) = match (consts.get(&n_val), consts.get(&m_val)) {
                    (Some(n), Some(m)) => (*n, *m),
                    _ => continue,
                };
                if !(1..=31).contains(&n) || n + m != 32 {
                    continue;
                }
                // the shifted-right operand must be the low half of `b_src`
                let same_low = if a_src == b_src {
                    clean(b_src, &bins, &consts, &small)
                } else {
                    match bins.get(&a_src) {
                        Some((BinOp::And, u, w)) => {
                            (*u == b_src && consts.get(w) == Some(&M32)) || (*w == b_src && consts.get(u) == Some(&M32))
                        }
                        _ => false,
                    }
                };
                if same_low {
                    hit = Some((b_src, n_val));
                    break;
                }
            }
            if let Some((src, n_val)) = hit {
                f.blocks[bi].insts[ii].op = Op::Bin(BinOp::Rotr32, src, n_val);
                bins.insert(d, (BinOp::Rotr32, src, n_val));
                changed += 1;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::Term;

    /// `(((x & M) >> n) | (x << (32 - n))) & M` as `rotr32` writes it.
    fn rotr32_func(n: i128) -> Func {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let x = f.param_val(0);
        let mask = f.push(0, FTy::U64, Op::Const(M32));
        let cn = f.push(0, FTy::U64, Op::Const(n));
        let cm = f.push(0, FTy::U64, Op::Const(32 - n));
        let lo = f.push(0, FTy::U64, Op::Bin(BinOp::And, x, mask));
        let r = f.push(0, FTy::U64, Op::Bin(BinOp::Shr, lo, cn));
        let l = f.push(0, FTy::U64, Op::Bin(BinOp::Shl, x, cm));
        let o = f.push(0, FTy::U64, Op::Bin(BinOp::Or, r, l));
        let out = f.push(0, FTy::U64, Op::Bin(BinOp::And, o, mask));
        f.set_term(0, Term::Ret(Some(out)));
        f
    }

    #[test]
    fn the_five_operation_rotate_becomes_one_op() {
        let mut f = rotr32_func(7);
        assert_eq!(run(&mut f), 1);
        let last = f.blocks[0].insts.last().unwrap();
        assert!(matches!(last.op, Op::Bin(BinOp::Rotr32, ..)), "{:?}", last.op);
    }

    #[test]
    fn mismatched_counts_are_left_alone() {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let x = f.param_val(0);
        let mask = f.push(0, FTy::U64, Op::Const(M32));
        let cn = f.push(0, FTy::U64, Op::Const(7));
        let cm = f.push(0, FTy::U64, Op::Const(24)); // 7 + 24 != 32
        let lo = f.push(0, FTy::U64, Op::Bin(BinOp::And, x, mask));
        let r = f.push(0, FTy::U64, Op::Bin(BinOp::Shr, lo, cn));
        let l = f.push(0, FTy::U64, Op::Bin(BinOp::Shl, x, cm));
        let o = f.push(0, FTy::U64, Op::Bin(BinOp::Or, r, l));
        let out = f.push(0, FTy::U64, Op::Bin(BinOp::And, o, mask));
        f.set_term(0, Term::Ret(Some(out)));
        assert_eq!(run(&mut f), 0);
    }

    #[test]
    fn an_unmasked_shift_right_of_an_unknown_value_is_not_a_rotate() {
        // `x >> n` without the mask keeps the upper 32 bits of x
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let x = f.param_val(0);
        let mask = f.push(0, FTy::U64, Op::Const(M32));
        let cn = f.push(0, FTy::U64, Op::Const(7));
        let cm = f.push(0, FTy::U64, Op::Const(25));
        let r = f.push(0, FTy::U64, Op::Bin(BinOp::Shr, x, cn));
        let l = f.push(0, FTy::U64, Op::Bin(BinOp::Shl, x, cm));
        let o = f.push(0, FTy::U64, Op::Bin(BinOp::Or, r, l));
        let out = f.push(0, FTy::U64, Op::Bin(BinOp::And, o, mask));
        f.set_term(0, Term::Ret(Some(out)));
        assert_eq!(run(&mut f), 0);
    }

    #[test]
    fn an_already_masked_input_needs_no_inner_mask() {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let x = f.param_val(0);
        let mask = f.push(0, FTy::U64, Op::Const(M32));
        let cn = f.push(0, FTy::U64, Op::Const(13));
        let cm = f.push(0, FTy::U64, Op::Const(19));
        let b = f.push(0, FTy::U64, Op::Bin(BinOp::And, x, mask));
        let r = f.push(0, FTy::U64, Op::Bin(BinOp::Shr, b, cn));
        let l = f.push(0, FTy::U64, Op::Bin(BinOp::Shl, b, cm));
        let o = f.push(0, FTy::U64, Op::Bin(BinOp::Or, l, r)); // operands swapped
        let out = f.push(0, FTy::U64, Op::Bin(BinOp::And, mask, o)); // mask first
        f.set_term(0, Term::Ret(Some(out)));
        assert_eq!(run(&mut f), 1);
    }

    #[test]
    fn folding_a_constant_rotate() {
        // 0x80000001 rotated right by 1 = 0xC0000000
        assert_eq!(crate::opt::fold_bin(FTy::U64, BinOp::Rotr32, 0x8000_0001, 1), Some(0xC000_0000));
    }
}
