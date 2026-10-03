// SPDX-License-Identifier: MPL-2.0
//! **ROUND REF4 -- `for x in array { ... }`** (element iteration).
//!
//! ```firn
//! var sum: i64 = 0
//! for x in values {        // values: [i64; 8]  (or a `&[i64; 8]` parameter)
//!     sum += x
//! }
//! ```
//!
//! The parser turns it into the counting loop the language already has:
//!
//! ```firn
//! for __each#L_C in 0 as usize..__each_len(values) {
//!     let x = values[__each#L_C]
//!     ...
//! }
//! ```
//!
//! `x` is an ordinary immutable `let`: a COPY of the element (so the element
//! type must be trivial -- a value with a `drop` cannot be moved out of an
//! array, and `moves.rs` says so). `__each_len(a)` is the one new intrinsic: the
//! length of an array (or of an array behind a reference parameter), a
//! constant of type `usize` -- nothing is computed at run time. Because the
//! array expression appears twice it must be a PLACE without calls: a name,
//! a field path, `*p`, or an index by a name / number. Anything else is a
//! syntax error that tells the author to name the array first.
//!
//! `continue` and `break` work as in every other `for` (the hidden counter is
//! raised in the step block of the loop).

use crate::ast::*;
use crate::diag::Span;
use crate::fir::{FTy, Val};
use crate::lower::Lower;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::Type;

/// The hidden intrinsic: the length of an array.
pub(crate) const LEN: &str = "__each_len";

/// Is `e` a place that may be evaluated twice (no calls, no side effects)?
fn place_ok(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Ident(_) => true,
        ExprKind::Field(b, ..) => place_ok(b),
        ExprKind::Unary(UnOp::Deref, b) => place_ok(b),
        ExprKind::Index(b, i) => {
            place_ok(b) && matches!(i.kind, ExprKind::Ident(_) | ExprKind::Int(_))
        }
        _ => false,
    }
}

/// A copy of a place with FRESH expression ids (the type table is keyed by id).
fn reid(p: &mut Parser, e: &Expr) -> Expr {
    let kind = match &e.kind {
        ExprKind::Ident(n) => ExprKind::Ident(n.clone()),
        ExprKind::Int(v) => ExprKind::Int(*v),
        ExprKind::Field(b, f, sp) => ExprKind::Field(Box::new(reid(p, b)), f.clone(), *sp),
        ExprKind::Unary(op, b) => ExprKind::Unary(*op, Box::new(reid(p, b))),
        ExprKind::Index(b, i) => ExprKind::Index(Box::new(reid(p, b)), Box::new(reid(p, i))),
        other => other.clone(),
    };
    p.mk(e.span, kind)
}

/// `for name in arr { body }` -- called by `Parser::for_stmt` once the body is
/// parsed. Returns the desugared counting loop.
pub(crate) fn build(
    p: &mut Parser,
    name: String,
    name_span: Span,
    arr: Expr,
    body: Block,
    start: Span,
) -> Stmt {
    if !place_ok(&arr) {
        p.dg.error_note(
            arr.span,
            "'for ... in' needs an array variable",
            "name the array first (`let a = ...`) and loop over `a`: the loop reads it twice, so it \
             must be a name, a field path or `*p`, never a call",
        );
        return Stmt::Error(start);
    }
    let idx = format!("__each#{}_{}", start.line, start.col);
    let arr_len = reid(p, &arr);
    let len = p.mk(arr.span, ExprKind::Call(LEN.to_string(), vec![arr_len], arr.span));
    let lit = p.mk(arr.span, ExprKind::Int(0));
    let zero = p.mk(
        arr.span,
        ExprKind::Cast(Box::new(lit), TypeExpr::Named("usize".to_string(), arr.span)),
    );
    let base = reid(p, &arr);
    let at = p.mk(name_span, ExprKind::Ident(idx.clone()));
    let elem = p.mk(arr.span, ExprKind::Index(Box::new(base), Box::new(at)));
    let bind = Stmt::Let { name, mutable: false, ty: None, init: elem, span: name_span };
    let mut stmts = Vec::with_capacity(body.stmts.len() + 1);
    stmts.push(bind);
    stmts.extend(body.stmts);
    let body = Block { stmts, span: body.span, end: body.end };
    Stmt::For {
        name: idx,
        start: zero,
        end: len,
        body,
        inclusive: false,
        name_span,
        span: start,
    }
}

/// `// HOOK foreach` in `sema::Checker::call` -- the type of `__each_len(a)`.
pub(crate) fn hook_call(ck: &mut Checker, name: &str, args: &[Expr], espan: Span) -> Option<Type> {
    if name != LEN {
        return None;
    }
    if args.len() != 1 {
        return Some(Type::Error);
    }
    let t = ck.expr(&args[0], None);
    let arr = match &t {
        Type::Array(..) => true,
        Type::Ptr { inner, .. } => matches!(**inner, Type::Array(..)),
        Type::Error => return Some(Type::Error),
        _ => false,
    };
    if !arr {
        ck.dg.error_note(
            args[0].span,
            format!("'for ... in' needs an array, found {}", ck.tcx.name_of(&t)),
            "the element loop walks a fixed size array `[T; N]`; for other sequences use an index loop",
        );
        let _ = espan;
        return Some(Type::Error);
    }
    Some(Type::Usize)
}

/// The length behind the type of the array expression.
fn length_of(t: &Type) -> Option<u64> {
    match t {
        Type::Array(_, n) => Some(*n),
        Type::Ptr { inner, .. } => match &**inner {
            Type::Array(_, n) => Some(*n),
            _ => None,
        },
        _ => None,
    }
}

/// Hook from `lower::lower_call`: the length is a constant.
pub(crate) fn lower_call(lo: &mut Lower, args: &[Expr], span: Span) -> Option<Option<Val>> {
    if args.len() != 1 {
        return lo.ice(span, "__each_len with wrong arity");
    }
    let n = match length_of(&lo.ty_of(&args[0])) {
        Some(n) => n,
        None => return lo.ice(span, "__each_len of something that is not an array"),
    };
    Some(Some(lo.constant(FTy::U64, n as i128)))
}
