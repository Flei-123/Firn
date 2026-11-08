use crate::ast::{BinOp, Expr, ExprKind, UnOp};
use crate::sema::Checker;
use crate::types::Type;

pub(crate) fn check_operands(ck: &mut Checker, op: BinOp, r: &Expr, lt: &Type) {
    if !lt.is_concrete_int() {
        return;
    }
    let Some(0) = ck.literal_index(r) else { return };
    match op {
        BinOp::Div => ck.dg.error(r.span, "division by the constant 0".to_string()),
        BinOp::Rem => ck.dg.error(r.span, "remainder by the constant 0".to_string()),
        _ => {}
    }
}

pub(crate) fn check_overflow(ck: &mut Checker, e: &Expr, ty: &Type) {
    if !ty.is_concrete_int() {
        return;
    }
    let ExprKind::Binary(op, _, _) = &e.kind else { return };
    if !matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) {
        return;
    }
    if ty.bits() == 64 && !ty.is_signed() {
        return;
    }
    let Some(value) = const_value(ck, e, ty) else { return };
    if !fits(ty, value) {
        ck.dg.error(e.span, format!("constant overflow: the result does not fit into the type {}", ck.tcx.name_of(ty)));
    }
}

fn const_value(ck: &Checker, e: &Expr, ty: &Type) -> Option<i128> {
    match &e.kind {
        ExprKind::Int(v) => Some(*v).filter(|v| leaf_fits(ty, *v)),
        ExprKind::Unary(UnOp::Neg, inner) => const_value(ck, inner, ty)?.checked_neg(),
        ExprKind::Ident(name) => ck.consts.get(name).filter(|(t, _)| !t.is_float())
            .map(|(_, v)| *v)
            .filter(|v| leaf_fits(ty, *v)),
        ExprKind::Binary(op, l, r) => {
            let (a, b) = (const_value(ck, l, ty)?, const_value(ck, r, ty)?);
            match op {
                BinOp::Add => a.checked_add(b),
                BinOp::Sub => a.checked_sub(b),
                BinOp::Mul => a.checked_mul(b),
                _ => None,
            }
        }
        _ => None,
    }
}

fn leaf_fits(ty: &Type, value: i128) -> bool {
    fits(ty, value) || fits(ty, -value)
}

fn fits(ty: &Type, value: i128) -> bool {
    let bits = ty.bits();
    if ty.is_signed() {
        let limit = 1i128 << (bits - 1);
        -limit <= value && value < limit
    } else {
        0 <= value && value < (1i128 << bits)
    }
}
