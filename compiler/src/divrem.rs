// SPDX-License-Identifier: MPL-2.0
//! `divrem` -- fuse `a / c` and `a % c` (same operand, same constant `c`).
//!
//! `x % 10` and `x / 10` next to each other (decimal printing, digit loops)
//! each cost one multiply-high sequence in the back end. The identity
//! `a % c == a - (a / c) * c` holds for truncating division, signed and
//! unsigned, for every constant `c` outside {0, 1, -1}. When a `Div` with the
//! same operand and constant exists in the function, the `Rem` is rewritten
//! to `a - (a / c) * c`; the common-subexpression pass then merges the two
//! divisions, leaving one multiply-high plus one multiply and one subtract.
//!
//! Powers of two are left alone (`peephole.rs` turns them into shifts and
//! masks). Without a matching `Div` nothing changes, so the pass never makes
//! a lone remainder slower. Switch: `--no-pass=divrem`.

use crate::fir::{BinOp, FTy, Func, Inst, Op, Val};
use std::collections::{HashMap, HashSet};

type Key = (u32, bool, Val, i128);

fn key(ty: FTy, a: Val, c: i128) -> Key {
    (ty.bits(), ty.signed(), a, c)
}

/// Rewrites matching remainders; returns how many.
pub fn run(f: &mut Func) -> usize {
    let mut consts: HashMap<Val, i128> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                consts.insert(d, *c);
            }
        }
    }
    let usable = |ty: FTy, c: i128| -> bool {
        if ty.is_float() || ty == FTy::Bool || ty == FTy::V128 || ty.bits() > 64 {
            return false;
        }
        let m = c.unsigned_abs();
        c != 0 && m != 1 && !m.is_power_of_two()
    };
    let mut divs: HashSet<Key> = HashSet::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let Op::Bin(BinOp::Div, a, d) = &i.op {
                if let Some(&c) = consts.get(d) {
                    if usable(i.ty, c) {
                        divs.insert(key(i.ty, *a, c));
                    }
                }
            }
        }
    }
    if divs.is_empty() {
        return 0;
    }
    struct Edit {
        block: usize,
        at: usize,
    }
    let mut edits: Vec<Edit> = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            if let (Some(_), Op::Bin(BinOp::Rem, a, d)) = (i.dst, &i.op) {
                if let Some(&c) = consts.get(d) {
                    if usable(i.ty, c) && divs.contains(&key(i.ty, *a, c)) {
                        edits.push(Edit { block: bi, at: ii });
                    }
                }
            }
        }
    }
    let n = edits.len();
    edits.sort_by(|x, y| (y.block, y.at).cmp(&(x.block, x.at)));
    for e in edits {
        let old = f.blocks[e.block].insts[e.at].clone();
        let (a, d) = match &old.op {
            Op::Bin(BinOp::Rem, a, d) => (*a, *d),
            _ => continue,
        };
        let dst = old.dst.unwrap();
        let ty = old.ty;
        let q = f.new_val_pub(ty);
        let m = f.new_val_pub(ty);
        let qi = Inst::like(Some(q), ty, Op::Bin(BinOp::Div, a, d), old.loc);
        let mi = Inst::like(Some(m), ty, Op::Bin(BinOp::Mul, q, d), old.loc);
        let si = Inst::like(Some(dst), ty, Op::Bin(BinOp::Sub, a, m), old.loc);
        let blk = &mut f.blocks[e.block].insts;
        blk[e.at] = si;
        blk.insert(e.at, mi);
        blk.insert(e.at, qi);
    }
    n
}
