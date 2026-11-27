// SPDX-License-Identifier: MPL-2.0
use crate::ast::*;
use crate::fx::HashMap;

const STR_TYPE: &str = "str";
const TO_STR_FN: &str = "io__fmt_string";
const BUILDER_TYPE: &str = "Fmt";

pub(crate) fn wants_str(t: Option<&TypeExpr>) -> bool {
    matches!(t, Some(TypeExpr::Named(n, _)) if n == STR_TYPE || n == crate::strtype::NAME_ALIAS)
}

fn is_builder_type(t: &TypeExpr) -> bool {
    match t {
        TypeExpr::Named(n, _) => n == BUILDER_TYPE || n.ends_with("__Fmt") || n.ends_with(".Fmt"),
        _ => false,
    }
}

pub(crate) fn is_builder(rets: &HashMap<String, TypeExpr>, e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Call(name, _, _) => rets.get(name.as_str()).is_some_and(is_builder_type),
        _ => false,
    }
}

pub(crate) fn wrap(next: &mut u32, e: &mut Expr) {
    let span = e.span;
    let id = *next;
    *next += 1;
    let placeholder = Expr { id, span, kind: ExprKind::Bool(false) };
    let inner = std::mem::replace(e, placeholder);
    e.kind = ExprKind::Call(TO_STR_FN.to_string(), vec![inner], span);
}

pub(crate) fn coerce(rets: &HashMap<String, TypeExpr>, next: &mut u32, e: &mut Expr, want: Option<&TypeExpr>) {
    if wants_str(want) && is_builder(rets, e) {
        wrap(next, e);
    }
}

pub(crate) fn coerce_operands(rets: &HashMap<String, TypeExpr>, next: &mut u32, op: BinOp, a: &mut Expr, b: &mut Expr) {
    if !matches!(op, BinOp::Add | BinOp::Eq | BinOp::Ne) {
        return;
    }
    for side in [a, b] {
        if is_builder(rets, side) {
            wrap(next, side);
        }
    }
}
