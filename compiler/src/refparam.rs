// SPDX-License-Identifier: MPL-2.0
//! **ROUND REF** -- reference parameters `x: &T` and `x: inout T`
//! (SPEC 3.2, "second class": parameters only).
//!
//! The parser turns the two spellings into the pointer types `*T` / `*mut T`
//! that stage 0 already has, and this module rewrites the BODY of the
//! function so that the parameter behaves like a reference:
//!
//! * `p.f` and (for an array target) `p[i]` dereference `p` automatically:
//!   they become `(*p).f` / `(*p)[i]`. Everywhere else the name is still the
//!   pointer, so `other(p)` passes the reference on.
//! * A `&T` parameter is read-only: an assignment, compound assignment or
//!   `++`/`--` whose target is rooted in it is an error ("use inout").
//! * Re-declaring the name (`let p`, `var p`, a `for` variable) is an error,
//!   because the rewrite would then be ambiguous.
//!
//! **Second class (ROUND BORROW1, r193):** a reference parameter may be used
//! only as `p.f` / `p[i]`, as `*p`, as an argument of a call (as it is, or as
//! `&p` / `inout p`). Copying it into a variable, a field or an array,
//! returning it, casting it or doing arithmetic on it is an error: that is
//! what stops a reference from outliving the call that lent it.
//!
//! What this deliberately does NOT do (the rest of the borrow checker,
//! ROADMAP r193): a reference handed to a call can still be turned into a raw
//! pointer by the callee, and exclusivity across statements is not tracked.

use crate::ast::*;
use crate::diag::{Diags, Span};

/// One reference parameter: name, `inout` (true) or `&` (false), and whether
/// the target type is an array (then indexing dereferences too).
#[derive(Clone, Debug)]
pub struct RefParam {
    pub name: String,
    pub inout: bool,
    pub array: bool,
}

struct Ctx<'a> {
    refs: &'a [RefParam],
    next_id: &'a mut u32,
    dg: &'a mut Diags,
}

/// Rewrites `body` in place. `next_id` is the parser's expression counter.
pub fn desugar(refs: &[RefParam], body: &mut Block, next_id: &mut u32, dg: &mut Diags) {
    if refs.is_empty() {
        return;
    }
    let mut cx = Ctx { refs, next_id, dg };
    block(&mut cx, body);
}

fn find<'a>(cx: &'a Ctx, name: &str) -> Option<&'a RefParam> {
    cx.refs.iter().find(|r| r.name == name)
}

fn mk(cx: &mut Ctx, span: Span, kind: ExprKind) -> Expr {
    let id = *cx.next_id;
    *cx.next_id += 1;
    Expr { id, span, kind }
}

fn deref_of(cx: &mut Ctx, e: Expr) -> Expr {
    let sp = e.span;
    mk(cx, sp, ExprKind::Unary(UnOp::Deref, Box::new(e)))
}

fn block(cx: &mut Ctx, b: &mut Block) {
    for s in b.stmts.iter_mut() {
        stmt(cx, s);
    }
}

fn shadow_check(cx: &mut Ctx, name: &str, span: Span) {
    if find(cx, name).is_some() {
        cx.dg.error_note(
            span,
            format!("'{}' is a reference parameter and cannot be declared again", name),
            "pick another name: the automatic dereference of a reference parameter \
             would otherwise be ambiguous",
        );
    }
}

/// The name of the read-only reference parameter a place expression is
/// rooted in, if any.
fn readonly_root(cx: &Ctx, e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(n) => match find(cx, n) {
            Some(r) if !r.inout => Some(n.clone()),
            _ => None,
        },
        ExprKind::Field(b, ..) | ExprKind::Index(b, _) => readonly_root(cx, b),
        ExprKind::Unary(UnOp::Deref, b) => readonly_root(cx, b),
        _ => None,
    }
}

fn write_check(cx: &mut Ctx, target: &Expr) {
    // Assigning to the bare name is the business of the ordinary
    // "parameters cannot be modified" check, not of this one.
    if matches!(target.kind, ExprKind::Ident(_)) {
        return;
    }
    if let Some(n) = readonly_root(cx, target) {
        cx.dg.error_note(
            target.span,
            format!("cannot write through the read-only reference parameter '{}'", n),
            "it is declared '&T'; declare it 'inout T' to modify the value",
        );
    }
}

/// Is `e` the bare name of a reference parameter?
fn is_ref_name(cx: &Ctx, e: &Expr) -> bool {
    matches!(&e.kind, ExprKind::Ident(n) if find(cx, n).is_some())
}

/// ROUND BORROW1: the bare name of a reference parameter used as a VALUE.
fn escape_error(cx: &mut Ctx, e: &Expr) {
    if let ExprKind::Ident(n) = &e.kind {
        cx.dg.error_note(
            e.span,
            format!("the reference parameter '{}' cannot be copied, stored or returned", n),
            "a reference is second class: use it as '{}.f' or '*{}', pass it to a call as it is, \
             or take it with '&{}' / 'inout {}'"
                .replace("{}", n),
        );
    }
}

/// An argument of a call: the bare name of a reference parameter is handed
/// on as it is.
fn arg(cx: &mut Ctx, a: &mut Expr) {
    if is_ref_name(cx, a) {
        return;
    }
    expr(cx, a);
}

fn stmt(cx: &mut Ctx, s: &mut Stmt) {
    match s {
        Stmt::Let { name, init, span, .. } => {
            shadow_check(cx, name, *span);
            expr(cx, init);
        }
        Stmt::AssignOp { target, value, .. } | Stmt::Assign { target, value, .. } => {
            // `p = ...` on the bare name is the business of the ordinary
            // "parameters cannot be modified" check.
            if !is_ref_name(cx, target) {
                expr(cx, target);
            }
            expr(cx, value);
            write_check(cx, target);
        }
        Stmt::Step { target, .. } => {
            expr(cx, target);
            write_check(cx, target);
        }
        Stmt::If { cond, then, els, .. } => {
            expr(cx, cond);
            block(cx, then);
            if let Some(e) = els {
                stmt(cx, e);
            }
        }
        Stmt::While { cond, body, .. } => {
            expr(cx, cond);
            block(cx, body);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                expr(cx, v);
            }
        }
        Stmt::For { name, start, end, body, name_span, .. } => {
            shadow_check(cx, name, *name_span);
            expr(cx, start);
            expr(cx, end);
            block(cx, body);
        }
        Stmt::Defer(inner, _, _) => stmt(cx, inner),
        Stmt::Expr(e) => expr(cx, e),
        Stmt::Block(b) => block(cx, b),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
    }
}

/// `p` as the base of `.f` / `[i]`: wrap it in a dereference.
fn auto_deref_base(cx: &mut Ctx, base: &mut Expr, indexing: bool) {
    if let ExprKind::Ident(n) = &base.kind {
        let hit = match find(cx, n) {
            Some(r) => !indexing || r.array,
            None => false,
        };
        if hit {
            let sp = base.span;
            let inner = std::mem::replace(base, Expr { id: 0, span: sp, kind: ExprKind::Int(0) });
            *base = deref_of(cx, inner);
            return;
        }
    }
    expr(cx, base);
}

fn expr(cx: &mut Ctx, e: &mut Expr) {
    match &mut e.kind {
        ExprKind::Field(b, ..) => auto_deref_base(cx, b, false),
        ExprKind::Index(b, i) => {
            auto_deref_base(cx, b, true);
            expr(cx, i);
        }
        ExprKind::IfElse(c, a, b) => {
            expr(cx, c);
            expr(cx, a);
            expr(cx, b);
        }
        ExprKind::Lambda(d) => block(cx, &mut d.body),
        ExprKind::Text(_, inner) => expr(cx, inner),
        ExprKind::Call(_, args, _) => {
            for a in args {
                arg(cx, a);
            }
        }
        ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
            for a in args {
                expr(cx, a);
            }
        }
        // `*p`: the explicit dereference of a reference parameter.
        ExprKind::Unary(UnOp::Deref, a) if is_ref_name(cx, a) => {}
        // `&p` / `inout p` on a reference parameter passes the reference on
        // (a reborrow); it must not become a pointer to the pointer. A
        // read-only `&T` cannot be handed on as `inout`.
        ExprKind::Unary(op @ (UnOp::AddrOf | UnOp::InoutOf), a)
            if matches!(&a.kind, ExprKind::Ident(n) if find(cx, n).is_some()) =>
        {
            let n = match &a.kind {
                ExprKind::Ident(n) => n.clone(),
                _ => unreachable!(),
            };
            if *op == UnOp::InoutOf && !find(cx, &n).map(|r| r.inout).unwrap_or(true) {
                cx.dg.error_note(
                    e.span,
                    format!("cannot pass the read-only reference parameter '{}' as 'inout'", n),
                    "it is declared '&T'; declare it 'inout T' to modify the value",
                );
            }
            e.kind = ExprKind::Ident(n);
        }
        ExprKind::Unary(_, a) | ExprKind::Cast(a, _) => expr(cx, a),
        ExprKind::Binary(_, a, b) | ExprKind::ArrayRepeat(a, b) => {
            expr(cx, a);
            expr(cx, b);
        }
        ExprKind::StructLit(_, fields, _) => {
            for (_, a, _) in fields {
                expr(cx, a);
            }
        }
        ExprKind::Float(..)
        | ExprKind::FloatF32(_)
        | ExprKind::Int(_)
        | ExprKind::Bool(_) => {}
        // A bare name that gets here is used as a value (ROUND BORROW1).
        ExprKind::Ident(_) => {
            if is_ref_name(cx, e) {
                escape_error(cx, e);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// "Exactly one `inout`" (SPEC 3.2): while a call holds modifiable access to a
// place, no other argument of the SAME call may touch an overlapping place.
// `f(inout a, a)`, `f(inout a, &a)`, `f(inout a, inout a)` and
// `f(inout a.x, a.x)` are refused; `f(inout a.x, a.y)` is fine (different
// fields). An index counts as the whole array. The check is purely syntactic
// and runs on every body, because `inout v` can appear in any call.

/// A place: root variable plus the path below it (`*` = dereference).
type Path = (String, Vec<String>);

/// The place an expression names, if it names one.
fn place_path(e: &Expr) -> Option<Path> {
    match &e.kind {
        ExprKind::Ident(n) => Some((n.clone(), Vec::new())),
        ExprKind::Field(b, f, _) => {
            let (r, mut p) = place_path(b)?;
            p.push(f.clone());
            Some((r, p))
        }
        ExprKind::Index(b, _) => place_path(b),
        ExprKind::Unary(UnOp::Deref, b) => {
            let (r, mut p) = place_path(b)?;
            p.push("*".to_string());
            Some((r, p))
        }
        _ => None,
    }
}

fn overlap(a: &Path, b: &Path) -> bool {
    if a.0 != b.0 {
        return false;
    }
    let n = a.1.len().min(b.1.len());
    a.1[..n] == b.1[..n]
}

/// Does `e` touch a place that overlaps `p`?
fn touches(e: &Expr, p: &Path) -> bool {
    if let Some(q) = place_path(e) {
        if overlap(&q, p) {
            return true;
        }
        // The index expressions inside the path still have to be looked at.
        return index_touches(e, p);
    }
    let mut hit = false;
    children(e, &mut |c| {
        if !hit && touches(c, p) {
            hit = true;
        }
    });
    hit
}

fn index_touches(e: &Expr, p: &Path) -> bool {
    match &e.kind {
        ExprKind::Index(b, i) => touches(i, p) || index_touches(b, p),
        ExprKind::Field(b, ..) | ExprKind::Unary(UnOp::Deref, b) => index_touches(b, p),
        _ => false,
    }
}

/// Calls `f` on every direct sub-expression.
fn children(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    match &e.kind {
        ExprKind::Field(b, ..) => f(b),
        ExprKind::Index(b, i) => {
            f(b);
            f(i);
        }
        ExprKind::IfElse(c, a, b) => {
            f(c);
            f(a);
            f(b);
        }
        ExprKind::Text(_, inner) => f(inner),
        ExprKind::Call(_, args, _) | ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
            for a in args {
                f(a);
            }
        }
        ExprKind::Unary(_, a) | ExprKind::Cast(a, _) => f(a),
        ExprKind::Binary(_, a, b) | ExprKind::ArrayRepeat(a, b) => {
            f(a);
            f(b);
        }
        ExprKind::StructLit(_, fields, _) => {
            for (_, a, _) in fields {
                f(a);
            }
        }
        // A closure body is walked as statements by the caller.
        ExprKind::Lambda(_)
        | ExprKind::Float(..)
        | ExprKind::FloatF32(_)
        | ExprKind::Int(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_) => {}
    }
}

/// Entry point: checks every call in `body`.
pub fn check_exclusive(body: &Block, dg: &mut Diags) {
    ex_block(body, dg);
}

fn ex_block(b: &Block, dg: &mut Diags) {
    for s in &b.stmts {
        ex_stmt(s, dg);
    }
}

fn ex_stmt(s: &Stmt, dg: &mut Diags) {
    match s {
        Stmt::Let { init, .. } => ex_expr(init, dg),
        Stmt::AssignOp { target, value, .. } | Stmt::Assign { target, value, .. } => {
            ex_expr(target, dg);
            ex_expr(value, dg);
        }
        Stmt::Step { target, .. } => ex_expr(target, dg),
        Stmt::If { cond, then, els, .. } => {
            ex_expr(cond, dg);
            ex_block(then, dg);
            if let Some(e) = els {
                ex_stmt(e, dg);
            }
        }
        Stmt::While { cond, body, .. } => {
            ex_expr(cond, dg);
            ex_block(body, dg);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                ex_expr(v, dg);
            }
        }
        Stmt::For { start, end, body, .. } => {
            ex_expr(start, dg);
            ex_expr(end, dg);
            ex_block(body, dg);
        }
        Stmt::Defer(inner, _, _) => ex_stmt(inner, dg),
        Stmt::Expr(e) => ex_expr(e, dg),
        Stmt::Block(b) => ex_block(b, dg),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
    }
}

fn ex_expr(e: &Expr, dg: &mut Diags) {
    if let ExprKind::Call(_, args, _) = &e.kind {
        for (i, a) in args.iter().enumerate() {
            let inner = match &a.kind {
                ExprKind::Unary(UnOp::InoutOf, inner) => inner,
                _ => continue,
            };
            let path = match place_path(inner) {
                Some(p) => p,
                None => continue,
            };
            for (j, other) in args.iter().enumerate() {
                if i != j && touches(other, &path) {
                    dg.error_note(
                        a.span,
                        format!(
                            "'{}' is passed as 'inout' and used again in the same call",
                            path.0
                        ),
                        "'inout' is exclusive access: while it is modifiable, nothing else may \
                         touch the same place -- use another variable or call twice",
                    );
                    break;
                }
            }
        }
    }
    if let ExprKind::Lambda(d) = &e.kind {
        ex_block(&d.body, dg);
        return;
    }
    children(e, &mut |c| ex_expr(c, dg));
}
