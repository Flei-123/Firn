use super::{place_root, Finding};
use crate::ast::{BinOp, Block, Expr, ExprKind, Program, Stmt, UnOp};
use crate::diag::Span;
use crate::refparam::children;

#[derive(Clone)]
struct Entry {
    ptr: String,
    root: String,
    take: Span,
    lent: Option<Span>,
}

#[derive(Clone, Default)]
struct State {
    entries: Vec<Entry>,
}

impl State {
    fn find(&self, ptr: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.ptr == ptr)
    }

    fn kill(&mut self, ptr: &str) {
        self.entries.retain(|e| e.ptr != ptr);
    }

    fn merge(&mut self, other: State) {
        for o in other.entries {
            match self.entries.iter_mut().find(|e| e.ptr == o.ptr && e.root == o.root) {
                Some(e) => e.lent = e.lent.or(o.lent),
                None => self.entries.push(o),
            }
        }
    }
}

struct Scan<'a> {
    out: &'a mut Vec<Finding>,
}

fn derive(e: &Expr, st: &State) -> Option<(String, Span, Option<Span>)> {
    match &e.kind {
        ExprKind::Cast(inner, _) => derive(inner, st),
        ExprKind::Binary(BinOp::Add | BinOp::Sub, a, b) => derive(a, st).or_else(|| derive(b, st)),
        ExprKind::Unary(UnOp::AddrOf | UnOp::InoutOf, inner) => match place_root(inner) {
            Some((name, false)) => Some((name.to_string(), e.span, None)),
            _ => None,
        },
        ExprKind::Ident(q) => st.find(q).map(|en| (en.root.clone(), en.take, en.lent)),
        _ => None,
    }
}

fn at(sp: Span) -> String {
    format!("{}:{}", sp.line, sp.col)
}

impl Scan<'_> {
    fn use_of(&mut self, e: &Expr, st: &State) {
        let name = match &e.kind {
            ExprKind::Ident(n) => n,
            _ => return,
        };
        let en = match st.find(name) {
            Some(en) => en,
            None => return,
        };
        let lent = match en.lent {
            Some(l) => l,
            None => return,
        };
        self.out.push(Finding {
            span: e.span,
            msg: format!(
                "the raw pointer '{}' is used after '{}' was passed as 'inout'",
                en.ptr, en.root
            ),
            note: format!(
                "'{}' points into '{}' ('&{}' at {}); 'inout {}' at {} lets the callee change or reuse it, so the pointer may be stale",
                en.ptr, en.root, en.root, at(en.take), en.root, at(lent)
            ),
            help: format!(
                "finish with '{}' before 'inout {}', or take the address again after the call",
                en.ptr, en.root
            ),
        });
    }

    fn call(&mut self, args: &[Expr], st: &mut State) {
        let mut lent: Vec<(String, Span)> = Vec::new();
        for a in args {
            if let ExprKind::Unary(UnOp::InoutOf, inner) = &a.kind {
                if let Some((root, _)) = place_root(inner) {
                    lent.push((root.to_string(), a.span));
                }
            }
        }
        for a in args {
            let q = match &a.kind {
                ExprKind::Ident(q) => q,
                _ => continue,
            };
            let en = match st.find(q) {
                Some(en) => en,
                None => continue,
            };
            if let Some((_, sp)) = lent.iter().find(|(r, _)| *r == en.root) {
                self.out.push(Finding {
                    span: a.span,
                    msg: format!(
                        "the raw pointer '{}' points into '{}', which is also passed as 'inout' in this call",
                        en.ptr, en.root
                    ),
                    note: format!(
                        "'{}' was taken from '&{}' at {}; 'inout {}' at {} must be the only way to reach it",
                        en.ptr, en.root, at(en.take), en.root, at(*sp)
                    ),
                    help: "pass only the pointer, or only 'inout', not both".to_string(),
                });
            }
        }
        for (root, sp) in lent {
            for en in st.entries.iter_mut().filter(|e| e.root == root) {
                en.lent = en.lent.or(Some(sp));
            }
        }
    }

    fn expr(&mut self, e: &Expr, st: &mut State) {
        match &e.kind {
            ExprKind::Lambda(_) => return,
            ExprKind::Ident(_) => self.use_of(e, st),
            _ => {}
        }
        let mut kids: Vec<&Expr> = Vec::new();
        children(e, &mut |c| kids.push(c));
        for c in kids {
            self.expr(c, st);
        }
        if let ExprKind::Call(_, args, _) = &e.kind {
            self.call(args, st);
        }
    }

    fn bind(&mut self, name: &str, value: &Expr, st: &mut State) {
        let derived = derive(value, st);
        st.kill(name);
        if let Some((root, take, lent)) = derived {
            st.entries.push(Entry { ptr: name.to_string(), root, take, lent });
        }
    }

    fn looped(&mut self, body: &Block, cond: Option<&Expr>, st: &mut State) {
        let before = st.clone();
        for _ in 0..2 {
            if let Some(c) = cond {
                self.expr(c, st);
            }
            self.block(body, st);
        }
        st.merge(before);
    }

    fn block(&mut self, b: &Block, st: &mut State) {
        for s in &b.stmts {
            self.stmt(s, st);
        }
    }

    fn stmt(&mut self, s: &Stmt, st: &mut State) {
        match s {
            Stmt::Let { name, init, .. } => {
                self.expr(init, st);
                self.bind(name, init, st);
            }
            Stmt::Assign { target, value, .. } => {
                self.expr(value, st);
                match &target.kind {
                    ExprKind::Ident(n) => self.bind(n, value, st),
                    _ => self.expr(target, st),
                }
            }
            Stmt::AssignOp { target, value, .. } => {
                self.expr(target, st);
                self.expr(value, st);
            }
            Stmt::Step { target, .. } => self.expr(target, st),
            Stmt::If { cond, then, els, .. } => {
                self.expr(cond, st);
                let mut a = st.clone();
                self.block(then, &mut a);
                if let Some(e) = els {
                    self.stmt(e, st);
                }
                st.merge(a);
            }
            Stmt::While { cond, body, .. } => self.looped(body, Some(cond), st),
            Stmt::For { start, end, body, .. } => {
                self.expr(start, st);
                self.expr(end, st);
                self.looped(body, None, st);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.expr(v, st);
                }
            }
            Stmt::Defer(inner, _, _) => self.stmt(inner, st),
            Stmt::Expr(e) => self.expr(e, st),
            Stmt::Block(b) => self.block(b, st),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
        }
    }
}

pub(super) fn check(prog: &Program, out: &mut Vec<Finding>) {
    for func in &prog.funcs {
        if crate::escape::has_allow_escape(func) {
            continue;
        }
        let mut st = State::default();
        Scan { out }.block(&func.body, &mut st);
    }
}
