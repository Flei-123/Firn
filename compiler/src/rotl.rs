// SPDX-License-Identifier: MPL-2.0
use crate::fir::{BinOp, FTy, Func, Op, Val};
use crate::fx::HashMap;

pub(crate) fn enabled() -> bool {
    std::env::var_os("FIRN_NO_ROTL").is_none()
}

fn width(ty: FTy) -> Option<i128> {
    match ty {
        FTy::U64 => Some(64),
        FTy::U32 => Some(32),
        _ => None,
    }
}

struct Tables {
    consts: HashMap<Val, i128>,
    bins: HashMap<Val, (BinOp, Val, Val, FTy)>,
}

impl Tables {
    fn build(f: &Func) -> Tables {
        let mut consts: HashMap<Val, i128> = HashMap::default();
        let mut bins: HashMap<Val, (BinOp, Val, Val, FTy)> = HashMap::default();
        for b in &f.blocks {
            for i in &b.insts {
                let d = match i.dst {
                    Some(d) => d,
                    None => continue,
                };
                match &i.op {
                    Op::Const(c) if !i.ty.is_float() && i.ty != FTy::V128 => {
                        consts.insert(d, *c);
                    }
                    Op::Bin(op, a, b2) => {
                        bins.insert(d, (*op, *a, *b2, i.ty));
                    }
                    _ => {}
                }
            }
        }
        Tables { consts, bins }
    }

    fn konst(&self, v: Val, w: i128) -> Option<i128> {
        self.consts.get(&v).map(|c| c.rem_euclid(w))
    }

    fn is_neg_of(&self, m: Val, k: Val, w: i128) -> bool {
        match self.bins.get(&m) {
            Some((BinOp::Sub, c, x, _)) if *x == k => self.konst(*c, w) == Some(0),
            _ => false,
        }
    }

    fn complementary(&self, k: Val, m: Val, w: i128) -> bool {
        if let (Some(a), Some(b)) = (self.konst(k, w), self.konst(m, w)) {
            return a != 0 && (a + b) % w == 0;
        }
        self.is_neg_of(m, k, w) || self.is_neg_of(k, m, w)
    }

    fn match_rotl(&self, op: BinOp, p: Val, q: Val, ty: FTy) -> Option<(Val, Val)> {
        let w = width(ty)?;
        for (l, r) in [(p, q), (q, p)] {
            let (x, k) = match self.bins.get(&l) {
                Some((BinOp::Shl, x, k, t)) if *t == ty => (*x, *k),
                _ => continue,
            };
            let (y, m) = match self.bins.get(&r) {
                Some((BinOp::Shr, y, m, t)) if *t == ty => (*y, *m),
                _ => continue,
            };
            if x != y || !self.complementary(k, m, w) {
                continue;
            }
            let disjoint_needed = op != BinOp::Or;
            if disjoint_needed && self.konst(k, w).is_none() {
                continue;
            }
            return Some((x, k));
        }
        None
    }
}

pub(crate) fn run(f: &mut Func) -> usize {
    let mut t = Tables::build(f);
    let mut changed = 0usize;
    for bi in 0..f.blocks.len() {
        for ii in 0..f.blocks[bi].insts.len() {
            let (d, op, p, q, ty) = {
                let i = &f.blocks[bi].insts[ii];
                match (&i.op, i.dst) {
                    (Op::Bin(op @ (BinOp::Or | BinOp::Xor | BinOp::Add), p, q), Some(d)) => (d, *op, *p, *q, i.ty),
                    _ => continue,
                }
            };
            if let Some((x, k)) = t.match_rotl(op, p, q, ty) {
                f.blocks[bi].insts[ii].op = Op::Bin(BinOp::Rotl, x, k);
                t.bins.insert(d, (BinOp::Rotl, x, k, ty));
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

    fn shape(ty: FTy, op: BinOp, var: bool, kc: i128, wc: i128) -> Func {
        let mut f = Func::new("t", vec![ty, ty], ty);
        let x = f.param_val(0);
        let (k, m) = if var {
            let k = f.param_val(1);
            let c = f.push(0, ty, Op::Const(wc));
            let m = f.push(0, ty, Op::Bin(BinOp::Sub, c, k));
            (k, m)
        } else {
            let k = f.push(0, ty, Op::Const(kc));
            let m = f.push(0, ty, Op::Const(wc - kc));
            (k, m)
        };
        let l = f.push(0, ty, Op::Bin(BinOp::Shl, x, k));
        let r = f.push(0, ty, Op::Bin(BinOp::Shr, x, m));
        let o = f.push(0, ty, Op::Bin(op, r, l));
        f.set_term(0, Term::Ret(Some(o)));
        f
    }

    fn last_is_rotl(f: &Func) -> bool {
        matches!(f.blocks[0].insts.last().unwrap().op, Op::Bin(BinOp::Rotl, ..))
    }

    #[test]
    fn variable_u64_rotate_is_recognized() {
        let mut f = shape(FTy::U64, BinOp::Or, true, 0, 64);
        assert_eq!(run(&mut f), 1);
        assert!(last_is_rotl(&f));
    }

    #[test]
    fn variable_u32_rotate_with_neg_count_is_recognized() {
        let mut f = shape(FTy::U32, BinOp::Or, true, 0, 0);
        assert_eq!(run(&mut f), 1);
    }

    #[test]
    fn constant_u64_rotate_with_xor_is_recognized() {
        let mut f = shape(FTy::U64, BinOp::Xor, false, 27, 64);
        assert_eq!(run(&mut f), 1);
    }

    #[test]
    fn variable_xor_is_not_a_rotate() {
        let mut f = shape(FTy::U64, BinOp::Xor, true, 0, 64);
        assert_eq!(run(&mut f), 0);
    }

    #[test]
    fn wrong_width_is_left_alone() {
        let mut f = shape(FTy::U64, BinOp::Or, true, 0, 32);
        assert_eq!(run(&mut f), 0);
        let mut g = shape(FTy::U64, BinOp::Or, false, 7, 60);
        assert_eq!(run(&mut g), 0);
    }

    #[test]
    fn signed_shift_is_not_a_rotate() {
        let mut f = shape(FTy::I64, BinOp::Or, true, 0, 64);
        assert_eq!(run(&mut f), 0);
    }

    #[test]
    fn folding_matches_rotate_left() {
        assert_eq!(crate::opt::fold_bin(FTy::U64, BinOp::Rotl, 0x8000_0000_0000_0001, 1), Some(3));
        assert_eq!(crate::opt::fold_bin(FTy::U32, BinOp::Rotl, 0x8000_0001, 33), Some(3));
    }
}
