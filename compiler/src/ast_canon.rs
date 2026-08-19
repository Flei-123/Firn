//! Kanonische Textform des AST — der **Maßstab** für den in Firn
//! geschriebenen Parser (`lib/firnc1/parser.fi`).
//!
//! ## Wozu eine zweite Ausgabe neben `--emit=ast`?
//!
//! `--emit=ast` ist Rusts `{:#?}`: an die Datenstruktur gebunden, mit
//! `Box`, `Some`/`None` und Feldnamen. Ein Parser in einer anderen Sprache
//! kann das nicht nachbauen, ohne Rusts Debug-Ausgabe nachzuäffen — und dann
//! prüft der Vergleich die Formatierung statt den Baum.
//!
//! Diese Form ist bewusst **sprachneutral**: geklammerte Listen, ein Knoten je
//! Zeile wäre unnötig, also eine Zeile je Deklaration. Zwei unabhängige Parser
//! können denselben Text erzeugen, wenn — und nur wenn — sie denselben Baum
//! gebaut haben.
//!
//! ## Was NICHT drinsteht
//!
//! Quellpositionen. Sie gehören zum Baum, aber ihre Zusammensetzung
//! (`Parser::join` über Teilausdrücke) ist eine eigene Verabredung; sie wird
//! getrennt geprüft, sobald der Parser steht. Ebenso fehlen die Erweiterungen,
//! die ihren Baum außerhalb von `Program` halten (`enum`/`match`,
//! Fehlerunionen, Generics, `gc class`, Attribute, `comptime`).

use crate::ast::*;
use crate::sema::TypeInfo;
use crate::types::TypeCtx;
use std::cell::RefCell;

thread_local! {
    /// Typtabelle fuer `--emit=typen`. Ist sie gesetzt, haengt an jeden
    /// Ausdruck sein Typ. Ein `thread_local` statt eines zusaetzlichen
    /// Parameters durch zwoelf Funktionen: die Ausgabe ist ein
    /// Fehlersuchwerkzeug, kein Teil des Compilerpfades.
    static TYPES: RefCell<Option<(Vec<crate::types::Type>, TypeCtx)>> = RefCell::new(None);
}

/// Wie `render`, aber mit dem Typ an jedem Ausdruck: `(int 5 :i32)`.
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
        ExprKind::Float(bits) => format!("(float {})", bits),
        ExprKind::Bool(b) => format!("(bool {})", b),
        ExprKind::Ident(n) => format!("(id {})", n),
        ExprKind::Unary(op, a) => format!(
            "(un {} {})",
            match op {
                UnOp::Neg => "-",
                UnOp::Not => "!",
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
    }
}
