// SPDX-License-Identifier: GPL-2.0-only
//! Canonical text form of the AST — the **yardstick** for the parser
//! that is written using Firn itself (`lib/firnc1/parser.fi`).
//!
//! ## Why a second rendering next to `--emit=ast`?
//!
//! `--emit=ast` is Rust's `{:#?}`: tied to the data structure, with `Box`,
//! `Some`/`None` and field names. A parser written for another language
//! cannot rebuild that without aping Rust's debug output — and then the
//! comparison checks the formatting rather than the tree.
//!
//! This form is deliberately **language neutral**: bracketed lists; one
//! node per line would be pointless, so one line per declaration. Two
//! independent parsers can produce the same text if — and only if — they
//! built the same tree.
//!
//! ## What is NOT part of it
//!
//! Source positions. They belong to the tree, but how they are composed
//! (`Parser::join` over subexpressions) is a convention of its own; it is
//! checked separately once the parser stands. Missing as well are the
//! extensions that keep their tree outside of `Program` (`enum`/`match`,
//! error unions, generics, `gc class`, attributes, `comptime`).

use crate::ast::*;
use crate::sema::TypeInfo;
use crate::types::TypeCtx;
use std::cell::RefCell;

thread_local! {
    /// Type table for `--emit=types`. Once it is set, every expression
    /// carries its type. A `thread_local` rather than one extra parameter
    /// threaded through twelve functions: this output is a troubleshooting
    /// tool, no part of the compiler path.
    static TYPES: RefCell<Option<(Vec<crate::types::Type>, TypeCtx)>> = RefCell::new(None);
}

/// Like `render`, but with the type at every expression: `(int 5 :i32)`.
pub fn render_typed(p: &Program, info: &TypeInfo) -> String {
    TYPES.with(|t| *t.borrow_mut() = Some((info.expr_types.clone(), info.tcx.clone())));
    let out = render(p);
    TYPES.with(|t| *t.borrow_mut() = None);
    out
}

fn ty_of(id: ExprId) -> Option<String> {
    TYPES.with(|t| {
        t.borrow().as_ref().map(|(tys, tcx)| {
            let ty = tys.get(id as usize).cloned().unwrap_or(crate::types::Type::Error);
            tcx.name_of(&ty)
        })
    })
}

pub fn render(p: &Program) -> String {
    let mut o = String::new();
    o.push_str("(program\n");
    if let Some((n, _)) = &p.profile {
        o.push_str(&format!("  (profile {})\n", n));
    }
    for i in &p.imports {
        o.push_str(&format!("  (import {} {})\n", i.path.join("."), i.alias));
    }
    for (n, _) in &p.exports {
        o.push_str(&format!("  (export {})\n", n));
    }
    for c in &p.consts {
        o.push_str(&format!("  (const {} {} {})\n", c.name, ty(&c.ty), ex(&c.value)));
    }
    for s in &p.structs {
        let mut f = String::new();
        for (n, t, _) in &s.fields {
            f.push_str(&format!(" (field {} {})", n, ty(t)));
        }
        o.push_str(&format!("  (struct {}{})\n", s.name, f));
    }
    for f in &p.funcs {
        let mut ps = String::new();
        for pa in &f.params {
            ps.push_str(&format!(" (param {} {})", pa.name, ty(&pa.ty)));
        }
        let r = match &f.ret {
            Some(t) => ty(t),
            None => "-".to_string(),
        };
        o.push_str(&format!("  (fn {} ({}) {} {})\n", f.name, ps.trim_start(), r, blk(&f.body)));
    }
    o.push_str(")\n");
    o
}

fn ty(t: &TypeExpr) -> String {
    match t {
        TypeExpr::Named(n, _) => n.clone(),
        TypeExpr::Ptr { mutable, inner, .. } => {
            format!("(ptr {} {})", if *mutable { "mut" } else { "const" }, ty(inner))
        }
        TypeExpr::Array { elem, len, .. } => format!("(arr {} {})", len, ty(elem)),
        // Round 58: a function type. `(fnty (arguments) result)`.
        TypeExpr::Fn { params, ret, .. } => {
            let ps: Vec<String> = params.iter().map(ty).collect();
            let r = match ret {
                Some(t) => ty(t),
                None => "void".to_string(),
            };
            format!("(fnty ({}) {})", ps.join(" "), r)
        }
    }
}

fn blk(b: &Block) -> String {
    let mut o = String::from("(blk");
    for s in &b.stmts {
        o.push(' ');
        o.push_str(&st(s));
    }
    o.push(')');
    o
}

fn opt_ty(t: &Option<TypeExpr>) -> String {
    match t {
        Some(t) => ty(t),
        None => "-".to_string(),
    }
}

fn st(s: &Stmt) -> String {
    match s {
        Stmt::Let { name, mutable, ty: t, init, .. } => format!(
            "({} {} {} {})",
            if *mutable { "var" } else { "let" },
            name,
            opt_ty(t),
            ex(init)
        ),
        Stmt::Assign { target, value, .. } => format!("(zuw {} {})", ex(target), ex(value)),
        // ROUND 70: the compound assignment and the step keep their OWN
        // shape - a canonical form that printed `x += 1` as `x = x + 1`
        // would hide exactly the difference this round is about.
        Stmt::AssignOp { target, op, value, .. } => {
            format!("(opassign {} {} {})", op.text(), ex(target), ex(value))
        }
        Stmt::Step { target, up, .. } => {
            format!("(step {} {})", if *up { "++" } else { "--" }, ex(target))
        }
        Stmt::If { cond, then, els, .. } => {
            let e = match els {
                Some(b) => st(b),
                None => "-".to_string(),
            };
            format!("(if {} {} {})", ex(cond), blk(then), e)
        }
        Stmt::While { cond, body, .. } => format!("(while {} {})", ex(cond), blk(body)),
        Stmt::Return { value, .. } => match value {
            Some(v) => format!("(ret {})", ex(v)),
            None => "(ret -)".to_string(),
        },
        Stmt::For { name, start, end, body, .. } => {
            format!("(for {} {} {} {})", name, ex(start), ex(end), blk(body))
        }
        Stmt::Break(_) => "(break)".to_string(),
        Stmt::Continue(_) => "(continue)".to_string(),
        Stmt::Defer(inner, is_err, _) => format!(
            "({} {})",
            if *is_err { "errdefer" } else { "defer" },
            st(inner)
        ),
        Stmt::Expr(e) => format!("(expr {})", ex(e)),
        Stmt::Block(b) => format!("(block {})", blk(b)),
        Stmt::Error(_) => "(error)".to_string(),
    }
}

fn ex(e: &Expr) -> String {
    let core = ex_core(e);
    match ty_of(e.id) {
        Some(t) => {
            let mut s = core;
            s.pop();
            format!("{} :{})", s, t)
        }
        None => core,
    }
}

fn ex_core(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(v) => format!("(int {})", v),
        ExprKind::Float(bits, _) => format!("(float {})", bits),
        // ROUND 71: the f32 literal carries its binary32 bit pattern.
        ExprKind::FloatF32(bits) => format!("(float32 {})", bits),
        ExprKind::Bool(b) => format!("(bool {})", b),
        ExprKind::Ident(n) => format!("(id {})", n),
        // Round 58: a closure literal. Its body is a block like any other.
        // NO serial number in the rendering: `lib/firnc1` numbers the
        // generated functions differently, and the number says nothing
        // about the tree.
        ExprKind::Lambda(d) => format!(
            "(closure {} ({}) {} {})",
            if d.heap { "gc" } else { "plain" },
            d.params
                .iter()
                .map(|p| format!("(param {} {})", p.name, ty(&p.ty)))
                .collect::<Vec<String>>()
                .join(" "),
            match &d.ret {
                Some(t) => ty(t),
                None => "-".to_string(),
            },
            blk(&d.body)
        ),
        ExprKind::Unary(op, a) => format!(
            "(un {} {})",
            match op {
                UnOp::Neg => "-",
                UnOp::Not => "!",
                UnOp::BitNot => "~",
                UnOp::AddrOf => "&",
                UnOp::Deref => "*",
            },
            ex(a)
        ),
        ExprKind::Binary(op, a, b) => format!("(bin {} {} {})", op.text(), ex(a), ex(b)),
        ExprKind::Field(b, n, _) => format!("(field {} {})", ex(b), n),
        ExprKind::Index(b, i) => format!("(idx {} {})", ex(b), ex(i)),
        ExprKind::Call(n, args, _) => {
            let mut o = format!("(call {}", n);
            for a in args {
                o.push(' ');
                o.push_str(&ex(a));
            }
            o.push(')');
            o
        }
        ExprKind::Syscall(args) => {
            let mut o = String::from("(syscall");
            for a in args {
                o.push(' ');
                o.push_str(&ex(a));
            }
            o.push(')');
            o
        }
        ExprKind::Cast(a, t) => format!("(as {} {})", ex(a), ty(t)),
        ExprKind::StructLit(n, fs, _) => {
            let mut o = format!("(slit {}", n);
            for (fname, fe, _) in fs {
                o.push_str(&format!(" (f {} {})", fname, ex(fe)));
            }
            o.push(')');
            o
        }
        ExprKind::ArrayLit(xs) => {
            let mut o = String::from("(alit");
            for a in xs {
                o.push(' ');
                o.push_str(&ex(a));
            }
            o.push(')');
            o
        }
        ExprKind::ArrayRepeat(v, n) => format!("(awdh {} {})", ex(v), ex(n)),
        // ROUND 70 — the text literal (strtype.rs). `w` = `u"…"`.
        ExprKind::Text(wide, inner) => format!("(text {} {})", *wide as u8, ex(inner)),
    }
}
