//! Monomorphisierung generischer Vorlagen (`L5`, Modul `types`).
//!
//! Laeuft zwischen Parser und Typpruefer: fuer jede im Quelltext benutzte
//! Typkombination entsteht eine konkrete Funktion bzw. ein konkreter Struct mit
//! dem Namen nach dem Vertrag `name__T1_T2` (siehe `sema_generic.rs`). Der
//! Typpruefer sieht danach nur noch gewoehnlichen, vollstaendig konkreten Code.
//!
//! Fehler dieser Stufe (falsche Anzahl Typargumente, nicht erfuellte
//! Anforderung, generischer Name ohne Typargumente) werden mit Zeile und Spalte
//! gemeldet; es gibt keinen Absturz.

use std::collections::{HashMap, HashSet};

use crate::ast::{Block, Expr, ExprKind, FnDecl, Program, Stmt, StructDecl, TypeExpr};
use crate::diag::{Diags, Span};
use crate::sema_generic::{
    self, instantiation, is_generic_fn, is_generic_struct, mangle, Bound, Instantiation,
};

/// Obergrenze gegen unendliche Auspraegungsketten (`Vec[Vec[Vec[..]]]`).
const MAX_INSTANCES: usize = 4096;

pub fn expand(prog: &mut Program, dg: &mut Diags) {
    let mut queue: Vec<(String, Instantiation)> = sema_generic::instantiations()
        .into_iter()
        .filter(|(_, i)| !i.abstrakt)
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
                "monomorphisierung: zu viele auspraegungen (rekursive generische verwendung?)",
            );
            break;
        }
        if inst.is_fn {
            expand_fn(prog, dg, &mangled, &inst, &mut queue, &mut next_id);
        } else {
            expand_struct(prog, dg, &mangled, &inst, &mut queue);
        }
    }
    prog.expr_count = next_id;

    // Generische Namen ohne Typargumente sind ein Fehler mit Zeile/Spalte.
    check_bare_uses(prog, dg);
}

// ------------------------------------------------------------- Auspraegungen

fn bind_params(
    dg: &mut Diags,
    params: &[sema_generic::TyParam],
    inst: &Instantiation,
    was: &str,
) -> Option<HashMap<String, TypeExpr>> {
    if params.len() != inst.args.len() {
        dg.error(
            inst.span,
            format!(
                "{} '{}' erwartet {} typargument(e), gefunden {}",
                was,
                inst.base,
                params.len(),
                inst.args.len()
            ),
        );
        return None;
    }
    let mut map = HashMap::new();
    for (p, a) in params.iter().zip(inst.args.iter()) {
        if !satisfies(a, p.bound) {
            dg.error_note(
                inst.span,
                format!(
                    "typargument '{}' erfuellt die anforderung '{}' des typparameters '{}' von '{}' nicht",
                    sema_generic::type_tag(a),
                    p.bound.name(),
                    p.name,
                    inst.base
                ),
                match p.bound {
                    Bound::Int => "erlaubt sind i8..i64, u8..u64, usize, isize",
                    Bound::Scalar => "erlaubt sind ganzzahlen, bool und zeiger",
                    Bound::Any => "kein typ erfuellt diese anforderung",
                },
            );
            return None;
        }
        map.insert(p.name.clone(), a.clone());
    }
    Some(map)
}

fn expand_fn(
    prog: &mut Program,
    dg: &mut Diags,
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
                format!("unbekannte generische funktion '{}'", inst.base),
            );
            return;
        }
    };
    let map = match bind_params(dg, &tpl.params, inst, "generische funktion") {
        Some(m) => m,
        None => return,
    };
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
    mangled: &str,
    inst: &Instantiation,
    queue: &mut Vec<(String, Instantiation)>,
) {
    let tpl = match sema_generic::struct_template(&inst.base) {
        Some(t) => t,
        None => {
            dg.error(
                inst.span,
                format!("unbekannter generischer struct '{}'", inst.base),
            );
            return;
        }
    };
    let map = match bind_params(dg, &tpl.params, inst, "generischer struct") {
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

fn satisfies(te: &TypeExpr, b: Bound) -> bool {
    match b {
        Bound::Any => true,
        Bound::Int => matches!(te, TypeExpr::Named(n, _) if is_int_name(n)),
        Bound::Scalar => match te {
            TypeExpr::Ptr { .. } => true,
            TypeExpr::Named(n, _) => is_int_name(n) || n == "bool",
            TypeExpr::Array { .. } => false,
        },
    }
}

fn is_int_name(n: &str) -> bool {
    matches!(
        n,
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
    }
}

/// Ersetzt einen Namen: Typparameter -> Argument, Auspraegungsname
/// (`Vec__T`) -> neuer Auspraegungsname (`Vec__i32`, dabei angemeldet).
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
    }
    let inst = instantiation(n)?;
    let args: Vec<TypeExpr> = inst.args.iter().map(|a| subst_ty(a, map, queue)).collect();
    let neu = mangle(&inst.base, &args);
    queue.push((
        neu.clone(),
        Instantiation {
            base: inst.base.clone(),
            args,
            span: sp,
            abstrakt: false,
            is_fn: inst.is_fn,
        },
    ));
    Some(TypeExpr::Named(neu, sp))
}

fn with_span(t: &TypeExpr, sp: Span) -> TypeExpr {
    match t {
        TypeExpr::Named(n, _) => TypeExpr::Named(n.clone(), sp),
        TypeExpr::Ptr { mutable, inner, .. } => TypeExpr::Ptr {
            mutable: *mutable,
            inner: inner.clone(),
            span: sp,
        },
        TypeExpr::Array { elem, len, .. } => TypeExpr::Array {
            elem: elem.clone(),
            len: *len,
            span: sp,
        },
    }
}

/// Name einer Funktion bzw. eines Struct-Literals im Rumpf umschreiben.
fn subst_call_name(
    n: &str,
    sp: Span,
    map: &HashMap<String, TypeExpr>,
    queue: &mut Vec<(String, Instantiation)>,
) -> String {
    match subst_name(n, sp, map, queue, true) {
        Some(TypeExpr::Named(neu, _)) => neu,
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
        Stmt::Assign { target, value, .. } => {
            subst_expr(target, map, queue);
            subst_expr(value, map, queue);
        }
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
        ExprKind::Int(_) | ExprKind::Bool(_) => {}
        ExprKind::Ident(_) => {}
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

// ------------------------------------------------------------ Neunummerierung

fn renumber_block(b: &mut Block, next: &mut u32) {
    for s in b.stmts.iter_mut() {
        renumber_stmt(s, next);
    }
}

fn renumber_stmt(s: &mut Stmt, next: &mut u32) {
    match s {
        Stmt::Defer(inner, _, _) => renumber_stmt(inner, next),
        Stmt::Let { init, .. } => renumber_expr(init, next),
        Stmt::Assign { target, value, .. } => {
            renumber_expr(target, next);
            renumber_expr(value, next);
        }
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

fn renumber_expr(e: &mut Expr, next: &mut u32) {
    e.id = *next;
    *next += 1;
    match &mut e.kind {
        ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
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

// --------------------------------------------------- generische Namen ohne []

fn check_bare_uses(prog: &Program, dg: &mut Diags) {
    let mut fehler: Vec<(Span, String)> = Vec::new();
    for f in &prog.funcs {
        for p in &f.params {
            check_bare_ty(&p.ty, &mut fehler);
        }
        if let Some(r) = &f.ret {
            check_bare_ty(r, &mut fehler);
        }
        check_bare_block(&f.body, &mut fehler);
    }
    for s in &prog.structs {
        for (_, te, _) in &s.fields {
            check_bare_ty(te, &mut fehler);
        }
    }
    for (sp, msg) in fehler {
        dg.error(sp, msg);
    }
}

fn check_bare_ty(te: &TypeExpr, out: &mut Vec<(Span, String)>) {
    match te {
        TypeExpr::Named(n, sp) => {
            if is_generic_struct(n) {
                out.push((
                    *sp,
                    format!("generischer struct '{}' braucht typargumente, z. B. '{}[i32]'", n, n),
                ));
            }
        }
        TypeExpr::Ptr { inner, .. } => check_bare_ty(inner, out),
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
        Stmt::Assign { target, value, .. } => {
            check_bare_expr(target, out);
            check_bare_expr(value, out);
        }
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
        ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
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
                        "generische funktion '{}' braucht typargumente, z. B. '{}[i32](..)'",
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
                    format!("generischer struct '{}' braucht typargumente, z. B. '{}[i32]'", name, name),
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
