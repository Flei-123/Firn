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
//! What this deliberately does NOT do (that is the borrow checker, ROADMAP
//! r18): the rule "exactly one `inout`" is not checked, and a reference may
//! still be copied into a raw pointer.

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

fn stmt(cx: &mut Ctx, s: &mut Stmt) {
    match s {
        Stmt::Let { name, init, span, .. } => {
            shadow_check(cx, name, *span);
            expr(cx, init);
        }
        Stmt::AssignOp { target, value, .. } | Stmt::Assign { target, value, .. } => {
            expr(cx, target);
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
        ExprKind::Call(_, args, _) | ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
            for a in args {
                expr(cx, a);
            }
        }
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
        | ExprKind::Bool(_)
        | ExprKind::Ident(_) => {}
    }
}
