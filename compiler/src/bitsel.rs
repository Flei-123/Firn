// SPDX-License-Identifier: MPL-2.0
//! **GAPS — bit-select and majority identities (x86-64 only).**
//!
//! SHA-2 style code spells the two round functions the long way:
//!
//! ```text
//!     ch  = (e & f) ^ (~e & g)            // 5 operations, 2 temporaries
//!     maj = (a & b) ^ (a & c) ^ (b & c)   // 5 operations, 3 temporaries
//! ```
//!
//! Equal bit for bit, cheaper:
//!
//! ```text
//!     ch  = g ^ (e & (f ^ g))             // 3 operations
//!     maj = (a & (b ^ c)) ^ (b & c)       // 4 operations, `b & c` is reused
//! ```
//!
//! Only when every intermediate value has exactly ONE reader (otherwise the
//! old operations stay alive and nothing is gained). `u32` and `u64` only.
//! The replaced operations die in `dce`. `FIRN_NO_BITSEL=1` switches it off.

use crate::fir::{BinOp, FTy, Func, Inst, Op, UnOp, Val};
use crate::fx::HashMap;

type Bins = HashMap<Val, (BinOp, Val, Val, FTy)>;

fn all_ones(consts: &HashMap<Val, i128>, ty: FTy, v: Val) -> bool {
    match consts.get(&v) {
        Some(c) => ty.truncate(*c) == ty.truncate(-1),
        None => false,
    }
}

/// `n` is `~x` (a `not`, or an xor with all ones) with exactly one reader: its operand.
fn not_of(n: Val, ty: FTy, bins: &Bins, nots: &HashMap<Val, Val>, consts: &HashMap<Val, i128>, uses: &[u32]) -> Option<Val> {
    if uses.get(n as usize).copied() != Some(1) {
        return None;
    }
    if let Some(x) = nots.get(&n) {
        return Some(*x);
    }
    match bins.get(&n) {
        Some((BinOp::Xor, a, b, t)) if *t == ty => {
            if all_ones(consts, ty, *b) {
                Some(*a)
            } else if all_ones(consts, ty, *a) {
                Some(*b)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn and_of(v: Val, ty: FTy, bins: &Bins, uses: &[u32]) -> Option<(Val, Val)> {
    if uses.get(v as usize).copied() != Some(1) {
        return None;
    }
    match bins.get(&v) {
        Some((BinOp::And, a, b, t)) if *t == ty => Some((*a, *b)),
        _ => None,
    }
}

/// `(x & y) ^ (~x & z)` as `(y, z, x)`.
fn match_ch(p: Val, q: Val, ty: FTy, bins: &Bins, nots: &HashMap<Val, Val>, consts: &HashMap<Val, i128>, uses: &[u32]) -> Option<(Val, Val, Val)> {
    let (pa, pb) = and_of(p, ty, bins, uses)?;
    let (qa, qb) = and_of(q, ty, bins, uses)?;
    for (n, z) in [(qa, qb), (qb, qa)] {
        if let Some(x) = not_of(n, ty, bins, nots, consts, uses) {
            if x == pa {
                return Some((pb, z, x));
            }
            if x == pb {
                return Some((pa, z, x));
            }
        }
    }
    None
}

/// `(a & b) ^ (a & c) ^ (b & c)` in any order: `(a, b, c, and_bc)`.
fn match_maj(p: Val, q: Val, ty: FTy, bins: &Bins, uses: &[u32]) -> Option<(Val, Val, Val, Val)> {
    for (inner, third) in [(p, q), (q, p)] {
        if uses.get(inner as usize).copied() != Some(1) {
            continue;
        }
        let (l1, l2) = match bins.get(&inner) {
            Some((BinOp::Xor, a, b, t)) if *t == ty => (*a, *b),
            _ => continue,
        };
        let leaves = [l1, l2, third];
        let mut pairs: Vec<(Val, Val)> = Vec::new();
        for l in leaves {
            match and_of(l, ty, bins, uses) {
                Some(pr) => pairs.push(pr),
                None => break,
            }
        }
        if pairs.len() != 3 {
            continue;
        }
        let mut vars: Vec<Val> = Vec::new();
        for (u, v) in &pairs {
            for x in [u, v] {
                if !vars.contains(x) {
                    vars.push(*x);
                }
            }
        }
        if vars.len() != 3 {
            continue;
        }
        // every unordered pair of variables exactly once
        let has = |x: Val, y: Val| pairs.iter().filter(|(u, v)| (*u == x && *v == y) || (*u == y && *v == x)).count();
        let (a, b, c) = (vars[0], vars[1], vars[2]);
        if has(a, b) != 1 || has(a, c) != 1 || has(b, c) != 1 {
            continue;
        }
        let and_bc = leaves
            .iter()
            .zip(pairs.iter())
            .find(|(_, (u, v))| (*u == b && *v == c) || (*u == c && *v == b))
            .map(|(l, _)| *l)?;
        return Some((a, b, c, and_bc));
    }
    None
}

/// Number of expressions rewritten.
pub(crate) fn run(f: &mut Func) -> usize {
    if std::env::var_os("FIRN_NO_BITSEL").is_some() {
        return 0;
    }
    let uses = crate::usecount::counts(f);
    let mut consts: HashMap<Val, i128> = HashMap::default();
    let mut bins: Bins = HashMap::default();
    let mut nots: HashMap<Val, Val> = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                match &i.op {
                    Op::Const(c) => {
                        consts.insert(d, *c);
                    }
                    Op::Bin(op, x, y) => {
                        bins.insert(d, (*op, *x, *y, i.ty));
                    }
                    Op::Un(UnOp::Not, x) => {
                        nots.insert(d, *x);
                    }
                    _ => {}
                }
            }
        }
    }
    let mut changed = 0usize;
    for bi in 0..f.blocks.len() {
        let old = std::mem::take(&mut f.blocks[bi].insts);
        let mut out: Vec<Inst> = Vec::with_capacity(old.len() + 4);
        for mut i in old {
            if let (Op::Bin(BinOp::Xor, p, q), Some(_)) = (&i.op, i.dst) {
                let (p, q, ty) = (*p, *q, i.ty);
                if ty == FTy::U32 || ty == FTy::U64 {
                    if let Some((y, z, x)) = match_ch(p, q, ty, &bins, &nots, &consts, &uses)
                        .or_else(|| match_ch(q, p, ty, &bins, &nots, &consts, &uses))
                    {
                        let t = f.new_val_pub(ty);
                        let u = f.new_val_pub(ty);
                        out.push(Inst::like(Some(t), ty, Op::Bin(BinOp::Xor, y, z), i.loc));
                        out.push(Inst::like(Some(u), ty, Op::Bin(BinOp::And, x, t), i.loc));
                        i.op = Op::Bin(BinOp::Xor, u, z);
                        changed += 1;
                    } else if let Some((a, b, c, and_bc)) = match_maj(p, q, ty, &bins, &uses) {
                        let t = f.new_val_pub(ty);
                        let u = f.new_val_pub(ty);
                        out.push(Inst::like(Some(t), ty, Op::Bin(BinOp::Xor, b, c), i.loc));
                        out.push(Inst::like(Some(u), ty, Op::Bin(BinOp::And, a, t), i.loc));
                        i.op = Op::Bin(BinOp::Xor, u, and_bc);
                        changed += 1;
                    }
                }
            }
            out.push(i);
        }
        f.blocks[bi].insts = out;
    }
    changed
}
