// SPDX-License-Identifier: MPL-2.0
use crate::ast::*;

const TEMP_PREFIX: &str = "__aw";

pub struct Hoist {
    temps: u32,
    next_id: u32,
}

impl Hoist {
    pub fn new(next_id: u32) -> Hoist {
        Hoist { temps: 0, next_id }
    }

    pub fn next_id(&self) -> u32 {
        self.next_id
    }

    pub fn stmt(&mut self, s: Stmt) -> Vec<Stmt> {
        let mut pre = Vec::new();
        let s = match s {
            Stmt::Let { name, mutable, ty, mut init, span } => {
                self.root(&mut init, &mut pre);
                Stmt::Let { name, mutable, ty, init, span }
            }
            Stmt::Assign { target, mut value, span } => {
                self.root(&mut value, &mut pre);
                Stmt::Assign { target, value, span }
            }
            Stmt::Expr(mut e) => {
                self.root(&mut e, &mut pre);
                Stmt::Expr(e)
            }
            Stmt::Return { value: Some(mut v), span } => {
                self.root(&mut v, &mut pre);
                Stmt::Return { value: Some(v), span }
            }
            Stmt::If { mut cond, then, els, span } => {
                self.operand(&mut cond, &mut pre);
                Stmt::If { cond, then, els, span }
            }
            other => other,
        };
        pre.push(s);
        pre
    }

    fn root(&mut self, e: &mut Expr, pre: &mut Vec<Stmt>) {
        match &mut e.kind {
            ExprKind::Field(inner, name, _) if name == crate::asyncfn::AWAIT => self.operand(inner, pre),
            _ => self.operand(e, pre),
        }
    }

    fn operand(&mut self, e: &mut Expr, pre: &mut Vec<Stmt>) {
        match &mut e.kind {
            ExprKind::Unary(_, a)
            | ExprKind::Cast(a, _)
            | ExprKind::Field(a, _, _)
            | ExprKind::Text(_, a)
            | ExprKind::Named(_, a, _) => self.operand(a, pre),
            ExprKind::Binary(op, a, b) => {
                self.operand(a, pre);
                if !matches!(op, BinOp::LAnd | BinOp::LOr) {
                    self.operand(b, pre);
                }
            }
            ExprKind::Index(a, b) | ExprKind::ArrayRepeat(a, b) => {
                self.operand(a, pre);
                self.operand(b, pre);
            }
            ExprKind::IfElse(c, _, _) => self.operand(c, pre),
            ExprKind::Call(_, xs, _) | ExprKind::Syscall(xs) | ExprKind::ArrayLit(xs) => {
                for x in xs.iter_mut() {
                    self.operand(x, pre);
                }
            }
            ExprKind::StructLit(_, fields, _) => {
                for (_, x, _) in fields.iter_mut() {
                    self.operand(x, pre);
                }
            }
            _ => {}
        }
        if is_await(e) {
            self.extract(e, pre);
        }
    }

    fn extract(&mut self, e: &mut Expr, pre: &mut Vec<Stmt>) {
        let name = format!("{}{}", TEMP_PREFIX, self.temps);
        self.temps += 1;
        let id = self.next_id;
        self.next_id += 1;
        let ident = Expr { id, span: e.span, kind: ExprKind::Ident(name.clone()) };
        let awaited = std::mem::replace(e, ident);
        let span = awaited.span;
        pre.push(Stmt::Let { name, mutable: false, ty: None, init: awaited, span });
    }
}

fn is_await(e: &Expr) -> bool {
    matches!(&e.kind, ExprKind::Field(_, name, _) if name == crate::asyncfn::AWAIT)
}
