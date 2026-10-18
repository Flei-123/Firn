// SPDX-License-Identifier: MPL-2.0
//! **r664 -- default and named arguments** (`docs/DEFAULT-ARGS.md`).
//!
//! ```text
//! fn connect(host: str, port: i32 = 80, tls: bool = false) -> i32 { .. }
//! connect("a")                 // port = 80, tls = false
//! connect("a", tls: true)      // named, skips port
//! connect(port: 1, host: "a")  // all named, any order
//! ```
//!
//! The parser records `Param.default` and `ExprKind::Named`; this pass runs
//! after the module merge and the monomorphization (every callee has its
//! final name then) and BEFORE the type checker. It turns every call into a
//! plain positional one: named arguments are put in parameter order, the
//! missing tail is filled with a copy of the default (fresh `ExprId`s).
//! Sema, lowering, the optimiser and the backends never see any of it.
//!
//! Scope: calls whose callee is a plain function known by name (also
//! `module.f` and generic instances). Method calls (`x.m(..)`) are resolved
//! by the type checker and are not rewritten here: a named argument on them
//! is an error.
//!
//! Evaluation order: arguments are evaluated in PARAMETER order, not in the
//! order they are written (the call is rewritten to positional form).

use crate::ast::*;
use crate::diag::{did_you_mean, nearest, Diags, Span};
use std::collections::{HashMap, HashSet};

struct Sig {
    names: Vec<String>,
    defaults: Vec<Option<Expr>>,
}

struct Cx<'a> {
    sigs: &'a HashMap<String, Sig>,
    next: u32,
    dg: &'a mut Diags,
}

/// Rewrites every call of `prog` in place.
pub fn apply(prog: &mut Program, dg: &mut Diags) {
    let mut sigs: HashMap<String, Sig> = HashMap::new();
    let consts: HashSet<String> = prog.consts.iter().map(|c| c.name.clone()).collect();
    for f in &prog.funcs {
        for p in &f.params {
            if let Some(d) = &p.default {
                check_const(d, &consts, dg);
            }
        }
        sigs.insert(
            f.name.clone(),
            Sig {
                names: f.params.iter().map(|p| p.name.clone()).collect(),
                defaults: f.params.iter().map(|p| p.default.clone()).collect(),
            },
        );
    }
    let mut cx = Cx { sigs: &sigs, next: prog.expr_count, dg };
    for f in prog.funcs.iter_mut() {
        block(&mut cx, &mut f.body);
        // The defaults themselves stay on the declaration; nothing reads them
        // after this pass.
        for p in f.params.iter_mut() {
            p.default = None;
        }
    }
    for c in prog.consts.iter_mut() {
        expr(&mut cx, &mut c.value);
    }
    for s in prog.statics.iter_mut() {
        expr(&mut cx, &mut s.value);
    }
    prog.expr_count = cx.next;
}

/// A default must be a constant expression: literals, `const` names, unary
/// and binary operators, casts, `size_of[..]()`.
fn check_const(e: &Expr, consts: &HashSet<String>, dg: &mut Diags) {
    let bad = |dg: &mut Diags, sp: Span| {
        dg.error_note(
            sp,
            "a default value must be a constant expression",
            "use literals, `const` names, operators and casts; no calls and no variables",
        );
    };
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Float(..) | ExprKind::FloatF32(_) => {}
        ExprKind::Text(_, _) => {}
        ExprKind::Ident(n) => {
            if !consts.contains(n) {
                bad(dg, e.span);
            }
        }
        ExprKind::Unary(op, a) => {
            if matches!(op, UnOp::Neg | UnOp::Not | UnOp::BitNot) {
                check_const(a, consts, dg);
            } else {
                bad(dg, e.span);
            }
        }
        ExprKind::Binary(_, a, b) => {
            check_const(a, consts, dg);
            check_const(b, consts, dg);
        }
        ExprKind::Cast(a, _) => check_const(a, consts, dg),
        ExprKind::Call(n, args, _) if args.is_empty() && n.starts_with("size_of") => {}
        _ => bad(dg, e.span),
    }
}

fn block(cx: &mut Cx, b: &mut Block) {
    for s in b.stmts.iter_mut() {
        stmt(cx, s);
    }
}

fn stmt(cx: &mut Cx, s: &mut Stmt) {
    match s {
        Stmt::Let { init, .. } => expr(cx, init),
        Stmt::AssignOp { target, value, .. } | Stmt::Assign { target, value, .. } => {
            expr(cx, target);
            expr(cx, value);
        }
        Stmt::Step { target, .. } => expr(cx, target),
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
        Stmt::For { start, end, body, .. } => {
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

fn expr(cx: &mut Cx, e: &mut Expr) {
    match &mut e.kind {
        ExprKind::Float(..)
        | ExprKind::FloatF32(_)
        | ExprKind::Int(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_) => {}
        ExprKind::Unary(_, a) | ExprKind::Cast(a, _) | ExprKind::Field(a, _, _) => expr(cx, a),
        ExprKind::Text(_, a) => expr(cx, a),
        ExprKind::Named(_, a, _) => expr(cx, a),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) | ExprKind::ArrayRepeat(a, b) => {
            expr(cx, a);
            expr(cx, b);
        }
        ExprKind::IfElse(c, a, b) => {
            expr(cx, c);
            expr(cx, a);
            expr(cx, b);
        }
        ExprKind::Syscall(xs) | ExprKind::ArrayLit(xs) => {
            for x in xs.iter_mut() {
                expr(cx, x);
            }
        }
        ExprKind::StructLit(_, fs, _) => {
            for (_, fe, _) in fs.iter_mut() {
                expr(cx, fe);
            }
        }
        ExprKind::Lambda(d) => block(cx, &mut d.body),
        ExprKind::Call(..) => call(cx, e),
    }
}

fn call(cx: &mut Cx, e: &mut Expr) {
    let call_span = e.span;
    let (name, args) = match &mut e.kind {
        ExprKind::Call(n, a, _) => (n.clone(), a),
        _ => return,
    };
    for a in args.iter_mut() {
        expr(cx, a);
    }
    let has_named = args.iter().any(|a| matches!(a.kind, ExprKind::Named(..)));
    let sig = match cx.sigs.get(&name) {
        Some(s) => s,
        None => {
            if has_named {
                cx.dg.error_note(
                    call_span,
                    format!("named arguments are not supported for the call to '{}'", pretty(&name)),
                    "they work on plain functions (also `module.f` and generic instances), not on method calls",
                );
            }
            return;
        }
    };
    let n = sig.names.len();
    let npos = args.iter().take_while(|a| !matches!(a.kind, ExprKind::Named(..))).count();
    // A positional argument behind a named one.
    if let Some(bad) = args.iter().skip(npos).find(|a| !matches!(a.kind, ExprKind::Named(..))) {
        cx.dg.error(bad.span, "positional argument after a named one");
        return;
    }
    if npos > n {
        return; // the type checker reports the wrong count
    }
    if !has_named && args.len() == n {
        return;
    }
    let mut slots: Vec<Option<Expr>> = (0..n).map(|_| None).collect();
    let mut ok = true;
    let old: Vec<Expr> = std::mem::take(args);
    for (i, a) in old.into_iter().enumerate() {
        if i < npos {
            slots[i] = Some(a);
            continue;
        }
        let (pname, val, nsp) = match a.kind {
            ExprKind::Named(pn, v, nsp) => (pn, *v, nsp),
            _ => continue,
        };
        match sig.names.iter().position(|x| *x == pname) {
            None => {
                ok = false;
                let note = match nearest(&pname, sig.names.iter().map(|s| s.as_str())) {
                    Some(c) => did_you_mean(&c),
                    None => format!("the parameters are: {}", sig.names.join(", ")),
                };
                cx.dg.error_note(
                    nsp,
                    format!("'{}' has no parameter named '{}'", pretty(&name), pname),
                    note,
                );
            }
            Some(k) => {
                if slots[k].is_some() {
                    ok = false;
                    cx.dg.error(
                        nsp,
                        format!("parameter '{}' of '{}' is given twice", pname, pretty(&name)),
                    );
                } else {
                    slots[k] = Some(val);
                }
            }
        }
    }
    if ok {
        let mut out: Vec<Expr> = Vec::with_capacity(n);
        for (k, slot) in slots.into_iter().enumerate() {
            match slot {
                Some(v) => out.push(v),
                None => match &sig.defaults[k] {
                    Some(d) => {
                        let mut c = d.clone();
                        crate::mono::renumber_expr(&mut c, &mut cx.next);
                        out.push(c);
                    }
                    None => {
                        ok = false;
                        cx.dg.error(
                            call_span,
                            format!(
                                "missing argument '{}' in the call to '{}'",
                                sig.names[k],
                                pretty(&name)
                            ),
                        );
                        break;
                    }
                },
            }
        }
        if ok {
            if let ExprKind::Call(_, a, _) = &mut e.kind {
                *a = out;
            }
        }
    }
}

/// The source spelling of a mangled name (`mod__f` -> `mod.f`).
fn pretty(n: &str) -> String {
    n.replacen("__", ".", 1)
}
