// SPDX-License-Identifier: MPL-2.0
use crate::fir::{BinOp, FTy, Func, Op, Val};
use crate::fx::HashMap;

pub(crate) fn enabled() -> bool {
    std::env::var_os("FIRN_NO_TRUNCMASK").is_none()
}

fn plain_int(ty: FTy) -> bool {
    matches!(
        ty,
        FTy::I8 | FTy::I16 | FTy::I32 | FTy::I64 | FTy::U8 | FTy::U16 | FTy::U32 | FTy::U64
    )
}

fn low_mask(bits: u32) -> i128 {
    (1i128 << bits) - 1
}

pub(crate) fn run(f: &mut Func) -> usize {
    let mut consts: HashMap<Val, i128> = HashMap::default();
    let mut ands: HashMap<Val, (Val, Val, FTy)> = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            match (&i.op, i.dst) {
                (Op::Const(c), Some(d)) if plain_int(i.ty) => {
                    consts.insert(d, *c);
                }
                (Op::Bin(BinOp::And, x, y), Some(d)) if plain_int(i.ty) => {
                    ands.insert(d, (*x, *y, i.ty));
                }
                _ => {}
            }
        }
    }
    if ands.is_empty() {
        return 0;
    }
    let mut changed = 0usize;
    for b in f.blocks.iter_mut() {
        for i in b.insts.iter_mut() {
            let to = i.ty;
            let (src, from) = match &i.op {
                Op::Cast { src, from } => (*src, *from),
                _ => continue,
            };
            if !plain_int(to) || !plain_int(from) || to.bits() >= from.bits() {
                continue;
            }
            let (x, y, aty) = match ands.get(&src) {
                Some(v) => *v,
                None => continue,
            };
            if aty != from {
                continue;
            }
            let m = low_mask(to.bits());
            let keep = |c: Val| consts.get(&c).map_or(false, |k| k & m == m);
            let inner = if keep(y) {
                x
            } else if keep(x) {
                y
            } else {
                continue;
            };
            i.op = Op::Cast { src: inner, from };
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::Term;

    fn shape(from: FTy, to: FTy, mask: i128, mask_left: bool) -> Func {
        let mut f = Func::new("t", vec![from], to);
        let x = f.param_val(0);
        let k = f.push(0, from, Op::Const(mask));
        let a = if mask_left {
            f.push(0, from, Op::Bin(BinOp::And, k, x))
        } else {
            f.push(0, from, Op::Bin(BinOp::And, x, k))
        };
        let c = f.push(0, to, Op::Cast { src: a, from });
        f.set_term(0, Term::Ret(Some(c)));
        f
    }

    fn cast_src(f: &Func) -> Val {
        match f.blocks[0].insts.last().unwrap().op {
            Op::Cast { src, .. } => src,
            _ => panic!("no cast"),
        }
    }

    #[test]
    fn low_byte_mask_before_u8_truncation_goes() {
        let mut f = shape(FTy::U64, FTy::U8, 255, false);
        assert_eq!(run(&mut f), 1);
        assert_eq!(cast_src(&f), f.param_val(0));
    }

    #[test]
    fn wider_mask_on_the_left_goes() {
        let mut f = shape(FTy::U32, FTy::U16, 0x1ffff, true);
        assert_eq!(run(&mut f), 1);
        assert_eq!(cast_src(&f), f.param_val(0));
    }

    #[test]
    fn partial_mask_stays() {
        let mut f = shape(FTy::U64, FTy::U8, 127, false);
        assert_eq!(run(&mut f), 0);
    }

    #[test]
    fn widening_cast_stays() {
        let mut f = shape(FTy::U8, FTy::U64, 255, false);
        assert_eq!(run(&mut f), 0);
    }
}
