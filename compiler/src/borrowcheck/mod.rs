mod alias;
mod refaddr;

use crate::ast::{Expr, ExprKind, UnOp};
use crate::diag::{Diag, Span};
use crate::sema::Checker;
use crate::ast::Program;

pub(crate) struct Finding {
    pub span: Span,
    pub msg: String,
    pub note: String,
    pub help: String,
}

pub(crate) fn place_root(e: &Expr) -> Option<(&str, bool)> {
    match &e.kind {
        ExprKind::Ident(n) => Some((n.as_str(), false)),
        ExprKind::Field(b, ..) | ExprKind::Index(b, _) => place_root(b),
        ExprKind::Unary(UnOp::Deref, b) => place_root(b).map(|(n, _)| (n, true)),
        _ => None,
    }
}

pub(crate) fn hook_check(ck: &mut Checker, prog: &Program) {
    if !crate::borrowmode::enabled() {
        return;
    }
    let mut found: Vec<Finding> = Vec::new();
    refaddr::check(prog, &mut found);
    alias::check(prog, &mut found);
    found.sort_by_key(|f| (f.span.file, f.span.line, f.span.col));
    found.dedup_by(|a, b| a.span.line == b.span.line && a.span.col == b.span.col && a.msg == b.msg);
    for f in found {
        ck.dg.report(Diag {
            msg: f.msg,
            span: f.span,
            label: "here".to_string(),
            note: Some(f.note),
            help: Some(f.help),
        });
    }
}
