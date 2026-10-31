use super::borrow::{Borrows, Probe};
use super::shape::Shape;
use super::walk::{binder_counts, call_count, children, stmt_blocks_mut, stmt_mentions, walk_expr, walk_expr_mut};
use crate::ast::{BinOp, Block, Expr, ExprId, ExprKind, FnDecl, Program, Stmt, TypeExpr, UnOp};
use crate::fx::HashMap;
use std::collections::{HashMap as StdMap, HashSet as StdSet};

const TMP_PREFIX: &str = "__own_t";

struct Rw<'a> {
    shapes: &'a HashMap<String, Shape>,
    fresh: &'a HashMap<String, String>,
    borrows: &'a Borrows,
    binders: StdMap<String, usize>,
    locals: StdSet<String>,
    next: &'a mut u32,
    tmps: usize,
    drop_name: &'static str,
}

fn find_candidate(rw: &Rw, e: &Expr, cond: bool, out: &mut Vec<(ExprId, Vec<usize>)>) {
    if let ExprKind::Call(name, args, _) = &e.kind {
        if let Some(flags) = rw.borrows.get(name) {
            let idx: Vec<usize> = args
                .iter()
                .enumerate()
                .filter(|(i, a)| {
                    flags.get(*i).copied().unwrap_or(false)
                        && matches!(&a.kind, ExprKind::Call(g, _, _) if rw.fresh.contains_key(g))
                })
                .map(|(i, _)| i)
                .collect();
            if !idx.is_empty() {
                if cond {
                    out.push((u32::MAX, Vec::new()));
                } else {
                    out.push((e.id, idx));
                }
            }
        }
    }
    match &e.kind {
        ExprKind::Binary(BinOp::LAnd | BinOp::LOr, a, b) => {
            find_candidate(rw, a, cond, out);
            find_candidate(rw, b, true, out);
        }
        ExprKind::IfElse(c, a, b) => {
            find_candidate(rw, c, cond, out);
            find_candidate(rw, a, true, out);
            find_candidate(rw, b, true, out);
        }
        _ => {
            for c in children(e) {
                find_candidate(rw, c, cond, out);
            }
        }
    }
}

fn simple(rw: &Rw, e: &Expr, hole: ExprId) -> bool {
    if e.id == hole {
        return true;
    }
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Float(..) | ExprKind::FloatF32(_) => true,
        ExprKind::Ident(n) => rw.locals.contains(n),
        ExprKind::Unary(op, a) => !matches!(op, UnOp::Deref | UnOp::AddrOf | UnOp::InoutOf) && simple(rw, a, hole),
        ExprKind::Binary(_, a, b) => simple(rw, a, hole) && simple(rw, b, hole),
        ExprKind::Cast(a, _) => simple(rw, a, hole),
        _ => false,
    }
}

fn fresh_id(rw: &mut Rw) -> u32 {
    let id = *rw.next;
    *rw.next += 1;
    id
}

fn hoist(rw: &mut Rw, s: &mut Stmt) -> Vec<Stmt> {
    let exprs: Vec<&Expr> = match s {
        Stmt::Let { init, .. } => vec![init],
        Stmt::Assign { target, value, .. } | Stmt::AssignOp { target, value, .. } => vec![target, value],
        Stmt::Expr(e) => vec![e],
        _ => return Vec::new(),
    };
    let mut found: Vec<(ExprId, Vec<usize>)> = Vec::new();
    for e in &exprs {
        find_candidate(rw, e, false, &mut found);
    }
    if found.len() != 1 || found[0].0 == u32::MAX {
        return Vec::new();
    }
    let (cid, idx) = found.remove(0);
    let total: usize = exprs.iter().map(|e| call_count(e)).sum();
    let mut inner = 0;
    let mut ok = true;
    for e in &exprs {
        walk_expr(e, &mut |x| {
            if x.id == cid {
                inner = call_count(x);
                if let ExprKind::Call(_, args, _) = &x.kind {
                    for (i, a) in args.iter().enumerate() {
                        if !idx.contains(&i) && !(simple(rw, a, u32::MAX) && call_count(a) == 0) {
                            ok = false;
                        }
                    }
                }
            }
        });
    }
    if !ok || inner != total || !exprs.iter().all(|e| simple(rw, e, cid)) {
        return Vec::new();
    }
    let mut lets: Vec<Stmt> = Vec::new();
    let mut targets: Vec<&mut Expr> = match s {
        Stmt::Let { init, .. } => vec![init],
        Stmt::Assign { target, value, .. } | Stmt::AssignOp { target, value, .. } => vec![target, value],
        Stmt::Expr(e) => vec![e],
        _ => return Vec::new(),
    };
    for root in targets.iter_mut() {
        walk_expr_mut(root, &mut |x| {
            if x.id != cid {
                return;
            }
            if let ExprKind::Call(_, args, _) = &mut x.kind {
                for i in idx.iter().copied() {
                    let name = format!("{}{}", TMP_PREFIX, rw.tmps);
                    rw.tmps += 1;
                    let span = args[i].span;
                    let id = *rw.next;
                    *rw.next += 1;
                    let repl = Expr { id, span, kind: ExprKind::Ident(name.clone()) };
                    let init = std::mem::replace(&mut args[i], repl);
                    rw.locals.insert(name.clone());
                    rw.binders.insert(name.clone(), 1);
                    lets.push(Stmt::Let { name, mutable: false, ty: None, init, span });
                }
            }
        });
    }
    lets
}

fn drop_stmt(rw: &mut Rw, var: &str, span: crate::diag::Span) -> Stmt {
    let ident = Expr { id: fresh_id(rw), span, kind: ExprKind::Ident(var.to_string()) };
    let cast = Expr { id: fresh_id(rw), span, kind: ExprKind::Cast(Box::new(ident), TypeExpr::Named("u64".to_string(), span)) };
    let call = Expr { id: fresh_id(rw), span, kind: ExprKind::Call(rw.drop_name.to_string(), vec![cast], span) };
    Stmt::Expr(call)
}

fn insert_drops(rw: &mut Rw, stmts: &mut Vec<Stmt>) {
    let mut ins: Vec<(usize, String, crate::diag::Span)> = Vec::new();
    for k in 0..stmts.len() {
        let Stmt::Let { name, mutable: false, init, span, .. } = &stmts[k] else { continue };
        let ExprKind::Call(g, _, _) = &init.kind else { continue };
        let Some(class) = rw.fresh.get(g) else { continue };
        let Some(shape) = rw.shapes.get(class) else { continue };
        if rw.binders.get(name).copied() != Some(1) {
            continue;
        }
        let probe = Probe { var: name, shape, borrows: rw.borrows };
        let mut last = k;
        for (j, s) in stmts.iter().enumerate().skip(k + 1) {
            if stmt_mentions(s, name) {
                last = j;
            }
        }
        if !stmts[k + 1..=last.max(k)].iter().all(|s| probe.stmt_ok(s)) && last > k {
            continue;
        }
        if matches!(stmts[last], Stmt::Return { .. } | Stmt::Break(_) | Stmt::Continue(_)) {
            continue;
        }
        ins.push((last + 1, name.clone(), *span));
    }
    ins.sort_by(|a, b| b.0.cmp(&a.0));
    for (at, name, span) in ins {
        let d = drop_stmt(rw, &name, span);
        stmts.insert(at, d);
    }
}

fn rewrite_block(rw: &mut Rw, b: &mut Block) {
    for s in b.stmts.iter_mut() {
        for sub in stmt_blocks_mut(s) {
            rewrite_block(rw, sub);
        }
    }
    let old = std::mem::take(&mut b.stmts);
    let mut out: Vec<Stmt> = Vec::with_capacity(old.len());
    for mut s in old {
        let lets = hoist(rw, &mut s);
        out.extend(lets);
        out.push(s);
    }
    insert_drops(rw, &mut out);
    b.stmts = out;
}

fn locals_of(f: &FnDecl, binders: &StdMap<String, usize>) -> StdSet<String> {
    let mut l: StdSet<String> = binders.keys().cloned().collect();
    l.extend(f.params.iter().map(|p| p.name.clone()));
    l
}

pub(crate) fn apply(prog: &mut Program) {
    if !super::enabled() || !crate::gc::has_classes() {
        return;
    }
    let shapes = super::shape::collect(prog);
    if shapes.is_empty() {
        return;
    }
    let uniq = super::shape::unique_names(prog);
    let fresh = super::fresh::compute(prog, &shapes, &uniq);
    if fresh.is_empty() {
        return;
    }
    let borrows = super::borrow::compute(prog, &shapes, &uniq);
    let drop_name = if super::mode() == super::Mode::Check { "__own_drop_tree_check" } else { "__own_drop_tree" };
    let mut next = prog.expr_count;
    for f in prog.funcs.iter_mut() {
        if !super::shape::plain(f) {
            continue;
        }
        let binders = binder_counts(&f.body);
        let locals = locals_of(f, &binders);
        let mut rw = Rw { shapes: &shapes, fresh: &fresh, borrows: &borrows, binders, locals, next: &mut next, tmps: 0, drop_name };
        rewrite_block(&mut rw, &mut f.body);
    }
    prog.expr_count = next;
}
