// SPDX-License-Identifier: MPL-2.0
use crate::ast::{BinOp, Expr, ExprKind, UnOp};
use crate::fir::{CmpOp, FTy, Op, Val};
use crate::impls;
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

const NEG_METHOD: &str = "neg";
const EQ_METHOD: &str = "eq";
const CMP_METHOD: &str = "cmp";

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Arith(&'static str),
    Eq,
    Ord,
}

fn kind(op: BinOp) -> Option<Kind> {
    match op {
        BinOp::Add => Some(Kind::Arith("add")),
        BinOp::Sub => Some(Kind::Arith("sub")),
        BinOp::Mul => Some(Kind::Arith("mul")),
        BinOp::Div => Some(Kind::Arith("div")),
        BinOp::Eq | BinOp::Ne => Some(Kind::Eq),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => Some(Kind::Ord),
        _ => None,
    }
}

fn method_of(k: Kind) -> &'static str {
    match k {
        Kind::Arith(m) => m,
        Kind::Eq => EQ_METHOD,
        Kind::Ord => CMP_METHOD,
    }
}

fn call_of(e: &Expr, method: &str, args: Vec<Expr>) -> Expr {
    Expr {
        id: e.id,
        span: e.span,
        kind: ExprKind::Call(format!("{}{}", impls::P_CALL, method), args, e.span),
    }
}

fn has_method(ck: &Checker, t: &Type, method: &str) -> bool {
    matches!(t, Type::Struct(_))
        && !crate::strtype::is_str_like(t)
        && impls::target_of(&ck.tcx, &ck.fns, method, t).is_some()
}

fn has_method_lowered(lw: &Lower, t: &Type, method: &str) -> bool {
    matches!(t, Type::Struct(_))
        && !crate::strtype::is_str_like(t)
        && impls::target(&lw.info, method, t).is_some()
}

pub(crate) fn probe_binary(ck: &Checker, op: BinOp, left: &Option<Type>) -> Option<Type> {
    let Kind::Arith(m) = kind(op)? else { return None };
    let lt = left.as_ref()?;
    if !has_method(ck, lt, m) {
        return None;
    }
    let (full, _) = impls::target_of(&ck.tcx, &ck.fns, m, lt)?;
    ck.fns.get(&full).map(|s| s.ret.clone())
}

pub(crate) fn hook_binary(ck: &mut Checker, e: &Expr, op: BinOp, l: &Expr, r: &Expr) -> Option<Type> {
    let k = kind(op)?;
    let method = method_of(k);
    let lt = ck.probe(l)?;
    if !has_method(ck, &lt, method) {
        return None;
    }
    let call = call_of(e, method, vec![l.clone(), r.clone()]);
    let t = ck.expr(&call, None);
    if t.is_error() {
        return Some(Type::Error);
    }
    match k {
        Kind::Arith(_) | Kind::Eq => Some(t),
        Kind::Ord => {
            if t != Type::I64 {
                ck.dg.error(
                    e.span,
                    format!(
                        "comparison between different types {} and {}",
                        ck.tcx.name_of(&t),
                        ck.tcx.name_of(&Type::I64)
                    ),
                );
                return Some(Type::Error);
            }
            Some(Type::Bool)
        }
    }
}

pub(crate) fn hook_unary(ck: &mut Checker, e: &Expr, op: UnOp, inner: &Expr) -> Option<Type> {
    if op != UnOp::Neg {
        return None;
    }
    let it = ck.probe(inner)?;
    if !has_method(ck, &it, NEG_METHOD) {
        return None;
    }
    let call = call_of(e, NEG_METHOD, vec![inner.clone()]);
    Some(ck.expr(&call, None))
}

pub(crate) fn desugar(lw: &Lower, e: &Expr) -> Option<Expr> {
    match &e.kind {
        ExprKind::Binary(op, l, r) => {
            let Some(Kind::Arith(m)) = kind(*op) else { return None };
            if !has_method_lowered(lw, &lw.ty_of(l), m) {
                return None;
            }
            Some(call_of(e, m, vec![(**l).clone(), (**r).clone()]))
        }
        ExprKind::Unary(UnOp::Neg, inner) => {
            if !has_method_lowered(lw, &lw.ty_of(inner), NEG_METHOD) {
                return None;
            }
            Some(call_of(e, NEG_METHOD, vec![(**inner).clone()]))
        }
        _ => None,
    }
}

pub(crate) fn lower_compare(
    lw: &mut Lower,
    e: &Expr,
    op: BinOp,
    a: &Expr,
    b: &Expr,
) -> Option<Option<Val>> {
    let k = kind(op)?;
    if !matches!(k, Kind::Eq | Kind::Ord) {
        return None;
    }
    let method = method_of(k);
    if !has_method_lowered(lw, &lw.ty_of(a), method) {
        return None;
    }
    let name = format!("{}{}", impls::P_CALL, method);
    let v = match lw.lower_call(&name, &[a.clone(), b.clone()], None, e.span)? {
        Some(v) => v,
        None => return Some(lw.ice(e.span, "operator method without a result")),
    };
    if k == Kind::Eq {
        return Some(Some(if op == BinOp::Ne { lw.push(FTy::Bool, Op::Un(crate::fir::UnOp::Not, v)) } else { v }));
    }
    let zero = lw.constant(FTy::I64, 0);
    let c = match op {
        BinOp::Lt => CmpOp::Lt,
        BinOp::Le => CmpOp::Le,
        BinOp::Gt => CmpOp::Gt,
        _ => CmpOp::Ge,
    };
    Some(Some(lw.push(FTy::Bool, Op::Cmp { op: c, ty: FTy::I64, a: v, b: zero })))
}
