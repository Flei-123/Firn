//! B3.1: fusion of `str` concatenation chains into one allocation.

use crate::ast::{self, Expr, ExprKind};
use crate::lower::Lower;

const FN_CONCAT3: &str = "__str_concat3";
const FN_CONCAT4: &str = "__str_concat4";
const FN_CONCAT4_TAIL: &str = "__str_concat4_tail";
const FN_CONCAT4_TAIL_CHECK: &str = "__str_concat4_tail_check";

fn str_add<'e>(lw: &Lower, e: &'e Expr) -> Option<(&'e Expr, &'e Expr)> {
    match &e.kind {
        ExprKind::Binary(ast::BinOp::Add, a, b) if crate::strtype::is_str_like(&lw.ty_of(e)) => Some((a, b)),
        _ => None,
    }
}

fn chain_len(lw: &Lower, e: &Expr) -> usize {
    let mut n = 1;
    let mut cur = e;
    while let Some((a, _)) = str_add(lw, cur) {
        n += 1;
        cur = a;
    }
    n
}

fn split<'e>(lw: &Lower, e: &'e Expr, max: usize) -> Vec<&'e Expr> {
    let mut rev: Vec<&Expr> = Vec::new();
    let mut cur = e;
    while rev.len() + 1 < max {
        match str_add(lw, cur) {
            Some((a, b)) => {
                rev.push(b);
                cur = a;
            }
            None => break,
        }
    }
    rev.push(cur);
    rev.reverse();
    rev
}

pub(crate) fn wants(lw: &Lower, e: &Expr) -> bool {
    super::enabled() && chain_len(lw, e) >= 3
}

fn callee(lw: &Lower, parts: &[&Expr]) -> &'static str {
    if parts.len() < 4 {
        return FN_CONCAT3;
    }
    if chain_len(lw, parts[0]) >= 3 {
        if super::mode() == super::Mode::Check {
            return FN_CONCAT4_TAIL_CHECK;
        }
        return FN_CONCAT4_TAIL;
    }
    FN_CONCAT4
}

pub(crate) fn lower_into(lw: &mut Lower, e: &Expr, addr: crate::fir::Val) -> Option<()> {
    let refs = split(lw, e, 4);
    let name = callee(lw, &refs);
    let parts: Vec<Expr> = refs.into_iter().cloned().collect();
    lw.lower_call(name, &parts, Some(addr), e.span).map(|_| ())
}

pub(crate) fn wants_assign(lw: &Lower, value: &Expr) -> bool {
    if !super::enabled() {
        return false;
    }
    let n = chain_len(lw, value);
    (2..=3).contains(&n)
}

pub(crate) fn lower_assign(lw: &mut Lower, target: &Expr, value: &Expr, addr: crate::fir::Val, span: crate::diag::Span) -> Option<()> {
    let mut parts: Vec<Expr> = vec![target.clone()];
    parts.extend(split(lw, value, 3).into_iter().cloned());
    let name = if parts.len() == 4 { FN_CONCAT4 } else { FN_CONCAT3 };
    lw.lower_call(name, &parts, Some(addr), span).map(|_| ())
}
