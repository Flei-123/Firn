use super::shape::{alloc_class, class_of, plain, Shape};
use super::walk::{binder_count, count_ident, immutable_lets, returns};
use crate::ast::{Expr, ExprKind, FnDecl, Program, TypeExpr};
use crate::fx::{HashMap, HashSet};

struct Cx<'a> {
    class: &'a str,
    shape: &'a Shape,
    fresh: &'a HashMap<String, String>,
    lets: HashMap<&'a str, &'a Expr>,
    consumed: Vec<&'a str>,
}

fn is_null(e: &Expr, class: &str) -> bool {
    match &e.kind {
        ExprKind::Cast(v, TypeExpr::Named(n, _)) => {
            matches!(v.kind, ExprKind::Int(0)) && n.strip_prefix(crate::gc::P_TY_PUB) == Some(class)
        }
        _ => false,
    }
}

fn alloc_ok<'a>(cx: &mut Cx<'a>, e: &'a Expr) -> bool {
    let ExprKind::Call(name, args, _) = &e.kind else { return false };
    if alloc_class(name) != Some(cx.class) || args.len() != 1 {
        return false;
    }
    let ExprKind::StructLit(_, fields, _) = &args[0].kind else { return false };
    for (fname, fe, _) in fields {
        if cx.shape.gc_fields.contains(fname) {
            if !fresh_expr(cx, fe, 0) {
                return false;
            }
        } else if !cx.shape.scalar_fields.contains(fname) {
            return false;
        }
    }
    true
}

fn fresh_expr<'a>(cx: &mut Cx<'a>, e: &'a Expr, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    if is_null(e, cx.class) {
        return true;
    }
    match &e.kind {
        ExprKind::Call(name, args, _) if name == crate::errors::CATCH_NAME && args.len() == 2 => {
            is_null(&args[1], cx.class) && alloc_ok(cx, &args[0])
        }
        ExprKind::Call(name, _, _) => cx.fresh.get(name).map(|c| c == cx.class).unwrap_or(false),
        ExprKind::Ident(x) => match cx.lets.get(x.as_str()).copied() {
            Some(init) => {
                cx.consumed.push(x.as_str());
                fresh_expr(cx, init, depth + 1)
            }
            None => false,
        },
        _ => false,
    }
}

fn valid(f: &FnDecl, class: &str, shape: &Shape, fresh: &HashMap<String, String>) -> bool {
    let lets: HashMap<&str, &Expr> = immutable_lets(&f.body).into_iter().collect();
    let rets = returns(&f.body);
    if rets.is_empty() {
        return false;
    }
    let mut cx = Cx { class, shape, fresh, lets, consumed: Vec::new() };
    for r in rets {
        if !fresh_expr(&mut cx, r, 0) {
            return false;
        }
    }
    let mut seen: HashSet<&str> = HashSet::default();
    for x in cx.consumed.iter().copied() {
        if !seen.insert(x) {
            return false;
        }
        if count_ident(&f.body, x) != 1 || binder_count(&f.body, x) != 1 || f.params.iter().any(|p| p.name == x) {
            return false;
        }
    }
    true
}

pub(crate) fn compute(prog: &Program, shapes: &HashMap<String, Shape>, uniq: &HashMap<String, usize>) -> HashMap<String, String> {
    let mut fresh: HashMap<String, String> = HashMap::default();
    for f in &prog.funcs {
        if let Some(c) = f.ret.as_ref().and_then(class_of) {
            if shapes.contains_key(c) && plain(f) && uniq.contains_key(&f.name) {
                fresh.insert(f.name.clone(), c.to_string());
            }
        }
    }
    loop {
        let mut drop_names: Vec<String> = Vec::new();
        for f in &prog.funcs {
            if let Some(c) = fresh.get(&f.name) {
                if !valid(f, c, &shapes[c], &fresh) {
                    drop_names.push(f.name.clone());
                }
            }
        }
        if drop_names.is_empty() {
            return fresh;
        }
        for n in drop_names {
            fresh.remove(&n);
        }
    }
}
