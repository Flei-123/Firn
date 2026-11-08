use crate::ast::{BinOp, Expr};
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
