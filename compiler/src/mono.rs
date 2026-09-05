// SPDX-License-Identifier: GPL-2.0-only
//! Monomorphization of generic templates (`L5`, module `types`).
//!
//! Runs between parser and type checker: for every type combination used by
//! the source text a concrete function or a concrete struct comes about,
//! named per the contract `name__T1_T2` (see `sema_generic.rs`).
//! After that the type checker sees plain, fully concrete code only.
//!
//! Errors of this stage (wrong number of type arguments, unmet bound,
//! generic name without type arguments) are reported with line and
//! column; there is no crash.

use std::collections::{HashMap, HashSet};

use crate::ast::{Block, Expr, ExprKind, FnDecl, Program, Stmt, StructDecl, TypeExpr};
use crate::diag::{Diags, Span};
use crate::sema_generic::{
    self, instantiation, is_generic_fn, is_generic_struct, mangle, Bound, Instantiation,
};

/// Upper bound against endless instantiation chains (`Vec[Vec[Vec[..]]]`).
const MAX_INSTANCES: usize = 4096;

pub fn expand(prog: &mut Program, dg: &mut Diags) {
    // All function names BEFORE instantiation — the bound check reads from
    // that which method of a type is missing (`T__m`). During instantiation
    // only monomorphized functions join; no interface is ever implemented
    // for those (`impl I for Vec__i32` cannot be written), so the snapshot
    // taken now suffices.
    let fnames: HashSet<String> = prog.funcs.iter().map(|f| f.name.clone()).collect();
    let mut queue: Vec<(String, Instantiation)> = sema_generic::instantiations()
        .into_iter()
        .filter(|(_, i)| !i.is_abstract)
        .collect();
    queue.reverse();
    let mut done: HashSet<String> = HashSet::new();
    let mut next_id = prog.expr_count;
    let mut count = 0usize;

    while let Some((mangled, inst)) = queue.pop() {
        if !done.insert(mangled.clone()) {
            continue;
        }
        count += 1;
        if count > MAX_INSTANCES {
            dg.error(
                inst.span,
                "monomorphization: too many instantiations (recursive generic use?)",
            );
            break;
        }
        if inst.is_fn {
            expand_fn(prog, dg, &fnames, &mangled, &inst, &mut queue, &mut next_id);
        } else {
            expand_struct(prog, dg, &fnames, &mangled, &inst, &mut queue);
        }
    }
    prog.expr_count = next_id;

    // Generic names without type arguments are an error with line/column.
    check_bare_uses(prog, dg);
}

// ------------------------------------------------------------ Instantiations

fn bind_params(
    dg: &mut Diags,
    fnames: &HashSet<String>,
    params: &[sema_generic::TyParam],
    inst: &Instantiation,
    what: &str,
) -> Option<HashMap<String, TypeExpr>> {
    if params.len() != inst.args.len() {
        dg.error(
            inst.span,
            format!(
                "{} '{}' expects {} type argument(s), found {}",
                what,
                inst.base,
                params.len(),
                inst.args.len()
            ),
        );
        return None;
    }
    let mut map = HashMap::new();
    for (p, a) in params.iter().zip(inst.args.iter()) {
        // EVERY bound must hold. What is reported is the FIRST violated one —
        // a cascade of follow-up messages about the same type argument says
        // nothing new.
        for b in &p.bounds {
            if !bound_ok(dg, fnames, a, b, &p.name, inst) {
                return None;
            }
        }
        map.insert(p.name.clone(), a.clone());
    }
    Some(map)
}

/// A single bound against one type argument. `true` = satisfied.
///
/// The three builtin bounds are decided by `satisfies` from the type shape
/// alone. An INTERFACE BOUND goes to `iface.rs`: only there is it written
/// which implementations exist and which method is missing.
fn bound_ok(
    dg: &mut Diags,
    fnames: &HashSet<String>,
    arg: &TypeExpr,
    b: &Bound,
    pname: &str,
    inst: &Instantiation,
) -> bool {
    if let Bound::Iface(i) = b {
        return crate::iface::bound_check(
            dg, fnames, arg, i, pname, &inst.base, inst.span,
        );
    }
    if satisfies(arg, b) {
        return true;
    }
    dg.error_note(
        inst.span,
        format!(
            "type argument '{}' does not satisfy the bound '{}' of the type parameter '{}' of '{}'",
            sema_generic::type_tag(arg),
            b.name(),
            pname,
            inst.base
        ),
        match b {
            Bound::Int => "allowed are i8..i64, u8..u64, usize, isize",
            Bound::Scalar => "allowed are integers, bool and pointers",
            _ => "no type satisfies this bound",
        },
    );
    false
}

fn expand_fn(
    prog: &mut Program,
    dg: &mut Diags,
    fnames: &HashSet<String>,
    mangled: &str,
    inst: &Instantiation,
    queue: &mut Vec<(String, Instantiation)>,
    next_id: &mut u32,
) {
    let tpl = match sema_generic::fn_template(&inst.base) {
        Some(t) => t,
        None => {
            dg.error(
                inst.span,
                format!("unknown generic function '{}'", inst.base),
            );
            return;
        }
    };
    let map = match bind_params(dg, fnames, &tpl.params, inst, "generic function") {
        Some(m) => m,
        None => return,
    };
    // ROUND 58 — an honest limit (fnval.rs): a closure literal inside a
    // template would be copied once per instantiation and would need one
    // capture record and one generated function per copy. Function
    // POINTERS work in a template without any restriction; only the
    // LITERAL is refused here, and it says so at the place where it stands.
    for sp in crate::fnval::spans_in(&tpl.decl.body) {
        dg.error_note(
            sp,
            "a closure literal inside a generic function is not supported",
            "round 58: pass the closure IN as a parameter of type 'fn(…)' \
             instead of building it inside the template",
        );
    }
    let mut decl: FnDecl = tpl.decl.clone();
    decl.name = mangled.to_string();
    for p in decl.params.iter_mut() {
        p.ty = subst_ty(&p.ty, &map, queue);
    }
    if let Some(r) = decl.ret.as_mut() {
        *r = subst_ty(r, &map, queue);
    }
    subst_block(&mut decl.body, &map, queue);
    renumber_block(&mut decl.body, next_id);
    prog.funcs.push(decl);
}

fn expand_struct(
    prog: &mut Program,
    dg: &mut Diags,
    fnames: &HashSet<String>,
    mangled: &str,
    inst: &Instantiation,
    queue: &mut Vec<(String, Instantiation)>,
) {
    let tpl = match sema_generic::struct_template(&inst.base) {
        Some(t) => t,
        None => {
            dg.error(
                inst.span,
                format!("unknown generic struct '{}'", inst.base),
            );
            return;
        }
    };
    let map = match bind_params(dg, fnames, &tpl.params, inst, "generic struct") {
        Some(m) => m,
        None => return,
    };
    let mut decl: StructDecl = tpl.decl.clone();
    decl.name = mangled.to_string();
    for (_, te, _) in decl.fields.iter_mut() {
        *te = subst_ty(te, &map, queue);
    }
    prog.structs.push(decl);
}

fn satisfies(te: &TypeExpr, b: &Bound) -> bool {
    match b {
        Bound::Any => true,
        Bound::Int => matches!(te, TypeExpr::Named(n, _) if is_int_name(n)),
        Bound::Scalar => match te {
            TypeExpr::Ptr { .. } => true,
            TypeExpr::Named(n, _) => is_int_name(n) || n == "bool",
            TypeExpr::Array { .. } => false,
            // Round 58: a function value is one word wide, so it is a scalar.
            TypeExpr::Fn { .. } => true,
        },
        // Interfaces are decided by `iface.rs`, not by the type shape.
        Bound::Iface(_) => false,
    }
}

fn is_int_name(n: &str) -> bool {
    matches!(
        crate::types::canon_name(n),
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize" | "isize"
    )
}

// ------------------------------------------------------------- Substitution

fn subst_ty(
    te: &TypeExpr,
    map: &HashMap<String, TypeExpr>,
    queue: &mut Vec<(String, Instantiation)>,
) -> TypeExpr {
    match te {
        TypeExpr::Named(n, sp) => subst_name(n, *sp, map, queue, false)
            .unwrap_or_else(|| TypeExpr::Named(n.clone(), *sp)),
        TypeExpr::Ptr { mutable, inner, span } => TypeExpr::Ptr {
            mutable: *mutable,
            inner: Box::new(subst_ty(inner, map, queue)),
            span: *span,
        },
        TypeExpr::Array { elem, len, span } => TypeExpr::Array {
            elem: Box::new(subst_ty(elem, map, queue)),
            len: *len,
            span: *span,
        },
        TypeExpr::Fn { params, ret, span } => TypeExpr::Fn {
            params: params.iter().map(|p| subst_ty(p, map, queue)).collect(),
            ret: ret.as_ref().map(|r| Box::new(subst_ty(r, map, queue))),
            span: *span,
        },
    }
}

/// Replaces one identifier: type parameter -> argument, instantiation name
/// (`Vec__T`) -> new instantiation name (`Vec__i32`, registered on the way).
fn subst_name(
    n: &str,
    sp: Span,
    map: &HashMap<String, TypeExpr>,
    queue: &mut Vec<(String, Instantiation)>,
    only_inst: bool,
) -> Option<TypeExpr> {
    if !only_inst {
        if let Some(t) = map.get(n) {
            return Some(with_span(t, sp));
        }
        // Round 53: `Gc[T]` and `GcWeak[T]` WITHIN ONE TEMPLATE.
        //
        // The parser turns those into the names `__gc#p:T` and `__gc#w:T`
        // (gc.rs::hook_type). Without this place the type resolution later
        // looks for a gc class called `T` and reports "unknown gc class
        // 'T'" — generic functions over Gc pointers were impossible that
        // way, and those are exactly what the type-safe surface of
        // `GcVec`/`GcMap` needs (`gcvec_append[T]`).
        //
        // Replaced is only what carries a NAME as its argument: `Gc[*mut u8]`
        // does not exist, the parser allows nothing but an identifier
        // there anyway.
        for pfx in [crate::gc::P_TY_PUB, crate::gc::P_WTYP_PUB] {
            if let Some(rest) = n.strip_prefix(pfx) {
                if let Some(TypeExpr::Named(concrete, _)) = map.get(rest) {
                    return Some(TypeExpr::Named(format!("{}{}", pfx, concrete), sp));
                }
                return None;
            }
        }
    }
    let inst = instantiation(n)?;
    let args: Vec<TypeExpr> = inst.args.iter().map(|a| subst_ty(a, map, queue)).collect();
    let new = mangle(&inst.base, &args);
    queue.push((
        new.clone(),
        Instantiation {
            base: inst.base.clone(),
            args,
            span: sp,
            is_abstract: false,
            is_fn: inst.is_fn,
        },
    ));
    Some(TypeExpr::Named(new, sp))
}

fn with_span(t: &TypeExpr, sp: Span) -> TypeExpr {
    match t {
        TypeExpr::Named(n, _) => TypeExpr::Named(n.clone(), sp),
        TypeExpr::Ptr { mutable, inner, .. } => TypeExpr::Ptr {
            mutable: *mutable,
            inner: inner.clone(),
            span: sp,
        },
        TypeExpr::Fn { params, ret, .. } => TypeExpr::Fn {
            params: params.clone(),
            ret: ret.clone(),
            span: sp,
        },
        TypeExpr::Array { elem, len, .. } => TypeExpr::Array {
            elem: elem.clone(),
            len: *len,
            span: sp,
        },
    }
}

/// Rewrite the name of a function or struct literal inside the body.
fn subst_call_name(
    n: &str,
    sp: Span,
    map: &HashMap<String, TypeExpr>,
    queue: &mut Vec<(String, Instantiation)>,
) -> String {
    // `size_of[T]()` inside a generic template: the type parameter sits
    // in the CALL NAME (`size_of$T`, see sizeof.rs) and has to be
    // substituted here as well — otherwise the type checker reports
    // "unknown type 'T'" as soon as the template is instantiated.
    if let Some(param) = n.strip_prefix("size_of$") {
        if let Some(TypeExpr::Named(concrete, _)) = map.get(param) {
            return format!("size_of${}", concrete);
        }
    }
    match subst_name(n, sp, map, queue, true) {
        Some(TypeExpr::Named(new, _)) => new,
        _ => n.to_string(),
    }
}

fn subst_block(b: &mut Block, map: &HashMap<String, TypeExpr>, queue: &mut Vec<(String, Instantiation)>) {
    for s in b.stmts.iter_mut() {
        subst_stmt(s, map, queue);
    }
}

fn subst_stmt(s: &mut Stmt, map: &HashMap<String, TypeExpr>, queue: &mut Vec<(String, Instantiation)>) {
    match s {
        Stmt::Defer(inner, _, _) => subst_stmt(inner, map, queue),
        Stmt::Let { ty, init, .. } => {
            if let Some(t) = ty.as_mut() {
                *t = subst_ty(t, map, queue);
            }
            subst_expr(init, map, queue);
        }
        Stmt::Assign { target, value, .. } | Stmt::AssignOp { target, value, .. } => {
            subst_expr(target, map, queue);
            subst_expr(value, map, queue);
        }
        // ROUND 70: the step has no value expression.
        Stmt::Step { target, .. } => subst_expr(target, map, queue),
        Stmt::If { cond, then, els, .. } => {
            subst_expr(cond, map, queue);
            subst_block(then, map, queue);
            if let Some(e) = els.as_mut() {
                subst_stmt(e, map, queue);
            }
        }
        Stmt::While { cond, body, .. } => {
            subst_expr(cond, map, queue);
            subst_block(body, map, queue);
        }
        Stmt::For { start, end, body, .. } => {
            subst_expr(start, map, queue);
            subst_expr(end, map, queue);
            subst_block(body, map, queue);
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value.as_mut() {
                subst_expr(e, map, queue);
            }
        }
        Stmt::Expr(e) => subst_expr(e, map, queue),
        Stmt::Block(b) => subst_block(b, map, queue),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
    }
}

fn subst_expr(e: &mut Expr, map: &HashMap<String, TypeExpr>, queue: &mut Vec<(String, Instantiation)>) {
    let sp = e.span;
    match &mut e.kind {
        ExprKind::Int(_) | ExprKind::Float(..) | ExprKind::FloatF32(_) | ExprKind::Bool(_) => {}
        ExprKind::Ident(_) => {}
        // ROUND 70: the text literal carries its array literal inside.
        ExprKind::Text(_, inner) => subst_expr(inner, map, queue),
        // Round 58: a closure inside a template is refused by
        // `check_bare_expr`; the types are substituted anyway, so that a
        // follow-up error stays readable.
        ExprKind::Lambda(d) => {
            for p in d.params.iter_mut() {
                p.ty = subst_ty(&p.ty, map, queue);
            }
            if let Some(t) = d.ret.as_mut() {
                *t = subst_ty(t, map, queue);
            }
            subst_block(&mut d.body, map, queue);
        }
        ExprKind::Unary(_, i) => subst_expr(i, map, queue),
        ExprKind::Binary(_, a, b) => {
            subst_expr(a, map, queue);
            subst_expr(b, map, queue);
        }
        ExprKind::Field(b, _, _) => subst_expr(b, map, queue),
        ExprKind::Index(b, i) => {
            subst_expr(b, map, queue);
            subst_expr(i, map, queue);
        }
        ExprKind::Call(name, args, _) => {
            *name = subst_call_name(name, sp, map, queue);
            for a in args.iter_mut() {
                subst_expr(a, map, queue);
            }
        }
        ExprKind::Syscall(args) => {
            for a in args.iter_mut() {
                subst_expr(a, map, queue);
            }
        }
        ExprKind::Cast(i, te) => {
            subst_expr(i, map, queue);
            *te = subst_ty(te, map, queue);
        }
        ExprKind::StructLit(name, fields, _) => {
            *name = subst_call_name(name, sp, map, queue);
            for (_, fe, _) in fields.iter_mut() {
                subst_expr(fe, map, queue);
            }
        }
        ExprKind::ArrayLit(els) => {
            for el in els.iter_mut() {
                subst_expr(el, map, queue);
            }
        }
        ExprKind::ArrayRepeat(v, n) => {
            subst_expr(v, map, queue);
            subst_expr(n, map, queue);
        }
    }
}

// ---------------------------------------------------------------- Renumbering

pub(crate) fn renumber_block(b: &mut Block, next: &mut u32) {
    for s in b.stmts.iter_mut() {
        renumber_stmt(s, next);
    }
}

fn renumber_stmt(s: &mut Stmt, next: &mut u32) {
    match s {
        Stmt::Defer(inner, _, _) => renumber_stmt(inner, next),
        Stmt::Let { init, .. } => renumber_expr(init, next),
        Stmt::Assign { target, value, .. } | Stmt::AssignOp { target, value, .. } => {
            renumber_expr(target, next);
            renumber_expr(value, next);
        }
        // ROUND 70: the step has no value expression.
        Stmt::Step { target, .. } => renumber_expr(target, next),
        Stmt::If { cond, then, els, .. } => {
            renumber_expr(cond, next);
            renumber_block(then, next);
            if let Some(e) = els.as_mut() {
                renumber_stmt(e, next);
            }
        }
        Stmt::While { cond, body, .. } => {
            renumber_expr(cond, next);
            renumber_block(body, next);
        }
        Stmt::For { start, end, body, .. } => {
            renumber_expr(start, next);
            renumber_expr(end, next);
            renumber_block(body, next);
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value.as_mut() {
                renumber_expr(e, next);
            }
        }
        Stmt::Expr(e) => renumber_expr(e, next),
        Stmt::Block(b) => renumber_block(b, next),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
    }
}

pub(crate) fn renumber_expr(e: &mut Expr, next: &mut u32) {
    e.id = *next;
    *next += 1;
    match &mut e.kind {
        ExprKind::Int(_) | ExprKind::Float(..) | ExprKind::FloatF32(_) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
        // ROUND 70: the text literal carries its array literal inside.
        ExprKind::Text(_, inner) => renumber_expr(inner, next),
        ExprKind::Lambda(d) => renumber_block(&mut d.body, next),
        ExprKind::Unary(_, i) => renumber_expr(i, next),
        ExprKind::Binary(_, a, b) => {
            renumber_expr(a, next);
            renumber_expr(b, next);
        }
        ExprKind::Field(b, _, _) => renumber_expr(b, next),
        ExprKind::Index(b, i) => {
            renumber_expr(b, next);
            renumber_expr(i, next);
        }
        ExprKind::Call(_, args, _) | ExprKind::Syscall(args) => {
            for a in args.iter_mut() {
                renumber_expr(a, next);
            }
        }
        ExprKind::Cast(i, _) => renumber_expr(i, next),
        ExprKind::StructLit(_, fields, _) => {
            for (_, fe, _) in fields.iter_mut() {
                renumber_expr(fe, next);
            }
        }
        ExprKind::ArrayLit(els) => {
            for el in els.iter_mut() {
                renumber_expr(el, next);
            }
        }
        ExprKind::ArrayRepeat(v, n) => {
            renumber_expr(v, next);
            renumber_expr(n, next);
        }
    }
}

// --------------------------------------------------- generic names without []

fn check_bare_uses(prog: &Program, dg: &mut Diags) {
    let mut err: Vec<(Span, String)> = Vec::new();
    for f in &prog.funcs {
        for p in &f.params {
            check_bare_ty(&p.ty, &mut err);
        }
        if let Some(r) = &f.ret {
            check_bare_ty(r, &mut err);
        }
        check_bare_block(&f.body, &mut err);
    }
    for s in &prog.structs {
        for (_, te, _) in &s.fields {
            check_bare_ty(te, &mut err);
        }
    }
    for (sp, msg) in err {
        dg.error(sp, msg);
    }
}

fn check_bare_ty(te: &TypeExpr, out: &mut Vec<(Span, String)>) {
    match te {
        TypeExpr::Named(n, sp) => {
            if is_generic_struct(n) {
                out.push((
                    *sp,
                    format!("generic struct '{}' needs type arguments, e.g. '{}[i32]'", n, n),
                ));
            }
        }
        TypeExpr::Ptr { inner, .. } => check_bare_ty(inner, out),
        TypeExpr::Fn { params, ret, .. } => {
            for p in params {
                check_bare_ty(p, out);
            }
            if let Some(r) = ret {
                check_bare_ty(r, out);
            }
        }
        TypeExpr::Array { elem, .. } => check_bare_ty(elem, out),
    }
}

fn check_bare_block(b: &Block, out: &mut Vec<(Span, String)>) {
    for s in &b.stmts {
        check_bare_stmt(s, out);
    }
}

fn check_bare_stmt(s: &Stmt, out: &mut Vec<(Span, String)>) {
    match s {
        Stmt::Defer(inner, _, _) => check_bare_stmt(inner, out),
        Stmt::Let { ty, init, .. } => {
            if let Some(t) = ty {
                check_bare_ty(t, out);
            }
            check_bare_expr(init, out);
        }
        Stmt::Assign { target, value, .. } | Stmt::AssignOp { target, value, .. } => {
            check_bare_expr(target, out);
            check_bare_expr(value, out);
        }
        // ROUND 70: the step has no value expression.
        Stmt::Step { target, .. } => check_bare_expr(target, out),
        Stmt::If { cond, then, els, .. } => {
            check_bare_expr(cond, out);
            check_bare_block(then, out);
            if let Some(e) = els {
                check_bare_stmt(e, out);
            }
        }
        Stmt::While { cond, body, .. } => {
            check_bare_expr(cond, out);
            check_bare_block(body, out);
        }
        Stmt::For { start, end, body, .. } => {
            check_bare_expr(start, out);
            check_bare_expr(end, out);
            check_bare_block(body, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value {
                check_bare_expr(e, out);
            }
        }
        Stmt::Expr(e) => check_bare_expr(e, out),
        Stmt::Block(b) => check_bare_block(b, out),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
    }
}

fn check_bare_expr(e: &Expr, out: &mut Vec<(Span, String)>) {
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Float(..) | ExprKind::FloatF32(_) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
        // ROUND 70: the text literal carries its array literal inside.
        ExprKind::Text(_, inner) => check_bare_expr(inner, out),
        // Round 58: a closure body carries types like any other body.
        ExprKind::Lambda(d) => {
            for p in &d.params {
                check_bare_ty(&p.ty, out);
            }
            if let Some(r) = &d.ret {
                check_bare_ty(r, out);
            }
            check_bare_block(&d.body, out);
        }
        ExprKind::Unary(_, i) => check_bare_expr(i, out),
        ExprKind::Binary(_, a, b) => {
            check_bare_expr(a, out);
            check_bare_expr(b, out);
        }
        ExprKind::Field(b, _, _) => check_bare_expr(b, out),
        ExprKind::Index(b, i) => {
            check_bare_expr(b, out);
            check_bare_expr(i, out);
        }
        ExprKind::Call(name, args, nspan) => {
            if is_generic_fn(name) {
                out.push((
                    *nspan,
                    format!(
                        "generic function '{}' needs type arguments, e.g. '{}[i32](..)'",
                        name, name
                    ),
                ));
            }
            for a in args {
                check_bare_expr(a, out);
            }
        }
        ExprKind::Syscall(args) => {
            for a in args {
                check_bare_expr(a, out);
            }
        }
        ExprKind::Cast(i, te) => {
            check_bare_expr(i, out);
            check_bare_ty(te, out);
        }
        ExprKind::StructLit(name, fields, nspan) => {
            if is_generic_struct(name) {
                out.push((
                    *nspan,
                    format!("generic struct '{}' needs type arguments, e.g. '{}[i32]'", name, name),
                ));
            }
            for (_, fe, _) in fields {
                check_bare_expr(fe, out);
            }
        }
        ExprKind::ArrayLit(els) => {
            for el in els {
                check_bare_expr(el, out);
            }
        }
        ExprKind::ArrayRepeat(v, n) => {
            check_bare_expr(v, out);
            check_bare_expr(n, out);
        }
    }
}
