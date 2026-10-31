use super::shape::{class_of, plain, Shape};
use super::walk::{binder_count, children, mentions, stmt_blocks, stmt_exprs};
use crate::ast::{BinOp, Block, Expr, ExprKind, FnDecl, Program, Stmt};
use crate::fx::HashMap;

pub(crate) type Borrows = HashMap<String, Vec<bool>>;

pub(crate) struct Probe<'a> {
    pub var: &'a str,
    pub shape: &'a Shape,
    pub borrows: &'a Borrows,
}

impl<'a> Probe<'a> {
    fn is_ref(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Ident(n) => n == self.var,
            ExprKind::Field(b, f, _) => {
                matches!(&b.kind, ExprKind::Ident(n) if n == self.var) && self.shape.gc_fields.contains(f)
            }
            _ => false,
        }
    }

    fn borrowed_arg(&self, callee: &str, i: usize) -> bool {
        self.borrows.get(callee).and_then(|v| v.get(i)).copied().unwrap_or(false)
    }

    pub(crate) fn expr_ok(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Ident(n) => n != self.var,
            ExprKind::Lambda(_) => false,
            ExprKind::Field(b, f, _) => match &b.kind {
                ExprKind::Ident(n) if n == self.var => self.shape.scalar_fields.contains(f),
                _ => self.expr_ok(b),
            },
            ExprKind::Call(name, args, _) => args.iter().enumerate().all(|(i, a)| {
                if self.is_ref(a) {
                    self.borrowed_arg(name, i)
                } else {
                    self.expr_ok(a)
                }
            }),
            ExprKind::Binary(BinOp::Eq | BinOp::Ne, a, b) => [a, b].iter().all(|x| self.is_ref(x) || self.expr_ok(x)),
            _ => children(e).into_iter().all(|c| self.expr_ok(c)),
        }
    }

    pub(crate) fn stmt_ok(&self, s: &Stmt) -> bool {
        match s {
            Stmt::Assign { target, value, .. } | Stmt::AssignOp { target, value, .. } => {
                !mentions(target, self.var) && self.expr_ok(value)
            }
            Stmt::Step { target, .. } => !mentions(target, self.var),
            Stmt::Defer(..) | Stmt::Error(_) => !super::walk::stmt_mentions(s, self.var),
            _ => stmt_exprs(s).into_iter().all(|e| self.expr_ok(e)) && stmt_blocks(s).into_iter().all(|b| self.block_ok(b)),
        }
    }

    pub(crate) fn block_ok(&self, b: &Block) -> bool {
        b.stmts.iter().all(|s| self.stmt_ok(s))
    }
}

fn param_ok(f: &FnDecl, i: usize, shapes: &HashMap<String, Shape>, borrows: &Borrows) -> bool {
    let p = &f.params[i];
    let Some(c) = class_of(&p.ty) else { return false };
    let Some(shape) = shapes.get(c) else { return false };
    if p.refk != 0 || binder_count(&f.body, &p.name) != 0 {
        return false;
    }
    Probe { var: &p.name, shape, borrows }.block_ok(&f.body)
}

pub(crate) fn compute(prog: &Program, shapes: &HashMap<String, Shape>, uniq: &HashMap<String, usize>) -> Borrows {
    let mut borrows: Borrows = HashMap::default();
    for f in &prog.funcs {
        if !plain(f) || !uniq.contains_key(&f.name) {
            continue;
        }
        let flags: Vec<bool> = f.params.iter().map(|p| class_of(&p.ty).map(|c| shapes.contains_key(c)).unwrap_or(false)).collect();
        if flags.iter().any(|b| *b) {
            borrows.insert(f.name.clone(), flags);
        }
    }
    loop {
        let mut changed = false;
        for f in &prog.funcs {
            let Some(flags) = borrows.get(&f.name).cloned() else { continue };
            for (i, on) in flags.iter().enumerate() {
                if *on && !param_ok(f, i, shapes, &borrows) {
                    borrows.get_mut(&f.name).unwrap()[i] = false;
                    changed = true;
                }
            }
        }
        if !changed {
            return borrows;
        }
    }
}
