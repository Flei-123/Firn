use crate::ast::{Block, Expr, ExprKind, Stmt};

pub(crate) fn children(e: &Expr) -> Vec<&Expr> {
    match &e.kind {
        ExprKind::Unary(_, a) | ExprKind::Field(a, _, _) | ExprKind::Named(_, a, _) | ExprKind::Cast(a, _) | ExprKind::Text(_, a) => vec![a],
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) | ExprKind::ArrayRepeat(a, b) => vec![a, b],
        ExprKind::Call(_, args, _) | ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => args.iter().collect(),
        ExprKind::StructLit(_, fields, _) => fields.iter().map(|f| &f.1).collect(),
        ExprKind::IfElse(a, b, c) => vec![a, b, c],
        ExprKind::Float(..) | ExprKind::FloatF32(_) | ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Ident(_) | ExprKind::Lambda(_) => Vec::new(),
    }
}

pub(crate) fn children_mut(e: &mut Expr) -> Vec<&mut Expr> {
    match &mut e.kind {
        ExprKind::Unary(_, a) | ExprKind::Field(a, _, _) | ExprKind::Named(_, a, _) | ExprKind::Cast(a, _) | ExprKind::Text(_, a) => vec![a],
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) | ExprKind::ArrayRepeat(a, b) => vec![a, b],
        ExprKind::Call(_, args, _) | ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => args.iter_mut().collect(),
        ExprKind::StructLit(_, fields, _) => fields.iter_mut().map(|f| &mut f.1).collect(),
        ExprKind::IfElse(a, b, c) => vec![a, b, c],
        ExprKind::Float(..) | ExprKind::FloatF32(_) | ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Ident(_) | ExprKind::Lambda(_) => Vec::new(),
    }
}

pub(crate) fn walk_expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    if let ExprKind::Lambda(d) = &e.kind {
        walk_block(&d.body, f);
        return;
    }
    for c in children(e) {
        walk_expr(c, f);
    }
}

pub(crate) fn walk_expr_mut(e: &mut Expr, f: &mut dyn FnMut(&mut Expr)) {
    f(e);
    for c in children_mut(e) {
        walk_expr_mut(c, f);
    }
}

pub(crate) fn stmt_exprs(s: &Stmt) -> Vec<&Expr> {
    match s {
        Stmt::Let { init, .. } => vec![init],
        Stmt::AssignOp { target, value, .. } | Stmt::Assign { target, value, .. } => vec![target, value],
        Stmt::Step { target, .. } => vec![target],
        Stmt::If { cond, .. } | Stmt::While { cond, .. } => vec![cond],
        Stmt::Return { value, .. } => value.iter().collect(),
        Stmt::For { start, end, .. } => vec![start, end],
        Stmt::Expr(e) => vec![e],
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Defer(..) | Stmt::Block(_) | Stmt::Error(_) => Vec::new(),
    }
}

pub(crate) fn stmt_blocks(s: &Stmt) -> Vec<&Block> {
    match s {
        Stmt::If { then, els, .. } => {
            let mut v = vec![then];
            let mut cur = els.as_deref();
            while let Some(c) = cur {
                match c {
                    Stmt::If { then, els, .. } => {
                        v.push(then);
                        cur = els.as_deref();
                    }
                    Stmt::Block(b) => {
                        v.push(b);
                        cur = None;
                    }
                    _ => cur = None,
                }
            }
            v
        }
        Stmt::While { body, .. } | Stmt::For { body, .. } => vec![body],
        Stmt::Block(b) => vec![b],
        _ => Vec::new(),
    }
}

pub(crate) fn walk_stmt(s: &Stmt, f: &mut dyn FnMut(&Expr)) {
    for e in stmt_exprs(s) {
        walk_expr(e, f);
    }
    if let Stmt::Defer(inner, _, _) = s {
        walk_stmt(inner, f);
    }
    for b in stmt_blocks(s) {
        walk_block(b, f);
    }
}

pub(crate) fn walk_block(b: &Block, f: &mut dyn FnMut(&Expr)) {
    for s in &b.stmts {
        walk_stmt(s, f);
    }
}

pub(crate) fn each_stmt(b: &Block, f: &mut dyn FnMut(&Stmt)) {
    for s in &b.stmts {
        f(s);
        if let Stmt::Defer(inner, _, _) = s {
            f(inner);
        }
        for sub in stmt_blocks(s) {
            each_stmt(sub, f);
        }
    }
}

pub(crate) fn count_ident(b: &Block, name: &str) -> usize {
    let mut n = 0;
    walk_block(b, &mut |e| {
        if matches!(&e.kind, ExprKind::Ident(x) if x == name) {
            n += 1;
        }
    });
    n
}

pub(crate) fn mentions(e: &Expr, name: &str) -> bool {
    let mut hit = false;
    walk_expr(e, &mut |x| {
        if matches!(&x.kind, ExprKind::Ident(n) if n == name) {
            hit = true;
        }
    });
    hit
}

pub(crate) fn stmt_mentions(s: &Stmt, name: &str) -> bool {
    let mut hit = false;
    walk_stmt(s, &mut |x| {
        if matches!(&x.kind, ExprKind::Ident(n) if n == name) {
            hit = true;
        }
    });
    hit
}

pub(crate) fn binder_count(b: &Block, name: &str) -> usize {
    let mut n = 0;
    each_stmt(b, &mut |s| match s {
        Stmt::Let { name: x, .. } | Stmt::For { name: x, .. } if x == name => n += 1,
        _ => {}
    });
    n
}

pub(crate) fn has_lambda(b: &Block) -> bool {
    let mut hit = false;
    walk_block(b, &mut |e| {
        if matches!(e.kind, ExprKind::Lambda(_)) {
            hit = true;
        }
    });
    hit
}

pub(crate) fn has_defer(b: &Block) -> bool {
    let mut hit = false;
    each_stmt(b, &mut |s| {
        if matches!(s, Stmt::Defer(..)) {
            hit = true;
        }
    });
    hit
}

pub(crate) fn returns(b: &Block) -> Vec<&Expr> {
    let mut v = Vec::new();
    fn go<'a>(b: &'a Block, v: &mut Vec<&'a Expr>) {
        for s in &b.stmts {
            if let Stmt::Return { value: Some(e), .. } = s {
                v.push(e);
            }
            if let Stmt::Defer(inner, _, _) = s {
                if let Stmt::Return { value: Some(e), .. } = &**inner {
                    v.push(e);
                }
            }
            for sub in stmt_blocks(s) {
                go(sub, v);
            }
        }
    }
    go(b, &mut v);
    v
}

pub(crate) fn immutable_lets(b: &Block) -> Vec<(&str, &Expr)> {
    let mut v = Vec::new();
    fn go<'a>(b: &'a Block, v: &mut Vec<(&'a str, &'a Expr)>) {
        for s in &b.stmts {
            if let Stmt::Let { name, mutable: false, init, .. } = s {
                v.push((name.as_str(), init));
            }
            for sub in stmt_blocks(s) {
                go(sub, v);
            }
        }
    }
    go(b, &mut v);
    v
}

pub(crate) fn stmt_blocks_mut(s: &mut Stmt) -> Vec<&mut Block> {
    match s {
        Stmt::If { then, els, .. } => {
            let mut v = vec![then];
            if let Some(e) = els.as_deref_mut() {
                v.extend(stmt_blocks_mut(e));
            }
            v
        }
        Stmt::While { body, .. } | Stmt::For { body, .. } => vec![body],
        Stmt::Block(b) => vec![b],
        _ => Vec::new(),
    }
}

pub(crate) fn binder_counts(b: &Block) -> std::collections::HashMap<String, usize> {
    let mut m: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    each_stmt(b, &mut |s| {
        if let Stmt::Let { name, .. } | Stmt::For { name, .. } = s {
            *m.entry(name.clone()).or_insert(0) += 1;
        }
    });
    m
}

pub(crate) fn call_count(e: &Expr) -> usize {
    let mut n = 0;
    walk_expr(e, &mut |x| {
        if matches!(x.kind, ExprKind::Call(..)) {
            n += 1;
        }
    });
    n
}
