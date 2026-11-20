use crate::ast::{Expr, ExprKind, Program};
use crate::diag::Span;
use crate::gc;
use crate::refparam::walk_exprs;
use crate::sema::Checker;
use crate::types::{Type, TypeCtx};
use std::sync::atomic::{AtomicBool, Ordering};

const SEND_SINKS: &[(&str, &[usize])] = &[
    ("thread_start", &[1]),
    ("jobs__spawn", &[2]),
    ("jobs__parallel_for", &[5]),
    ("pool__submit", &[2]),
    ("chan__chan_send1", &[1]),
    ("chan__chan_send", &[1]),
    ("chan__chan_try_send", &[1]),
    ("jobs__parallel_each", &[3]),
    ("jobs__group_go", &[6]),
    ("pool__run_all", &[2]),
];

const MAX_DEPTH: u32 = 32;
const MAX_POINTER_HOPS: u32 = 2;

pub(crate) const ALLOWED: &str = "off, on";

static ENABLED: AtomicBool = AtomicBool::new(false);

pub(crate) fn parse(v: &str) -> Option<bool> {
    match v {
        "off" => Some(false),
        "on" => Some(true),
        _ => None,
    }
}

pub(crate) fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

fn sink_args(name: &str) -> Option<&'static [usize]> {
    SEND_SINKS.iter().find(|(n, _)| *n == name).map(|(_, args)| *args)
}

fn strip_casts(e: &Expr) -> &Expr {
    match &e.kind {
        ExprKind::Cast(inner, _) => strip_casts(inner),
        _ => e,
    }
}

fn holds_gc(tcx: &TypeCtx, t: &Type, depth: u32, hops: u32) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    if gc::is_gc_ref(t) {
        return true;
    }
    match t {
        Type::Struct(i) => tcx
            .structs
            .get(*i)
            .is_some_and(|s| s.fields.iter().any(|f| holds_gc(tcx, &f.ty, depth + 1, hops))),
        Type::Array(inner, _) => holds_gc(tcx, inner, depth + 1, hops),
        Type::Ptr { inner, .. } => hops < MAX_POINTER_HOPS && holds_gc(tcx, inner, depth + 1, hops + 1),
        _ => false,
    }
}

pub(crate) fn hook_check(ck: &mut Checker, prog: &Program) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let mut errors: Vec<(Span, String)> = Vec::new();
    for f in &prog.funcs {
        if f.extern_info.is_some() {
            continue;
        }
        walk_exprs(&f.body, &mut |e| {
            let ExprKind::Call(name, args, _) = &e.kind else { return };
            let Some(indexes) = sink_args(name) else { return };
            for i in indexes {
                let Some(arg) = args.get(*i) else { continue };
                let source = strip_casts(arg);
                let Some(t) = ck.expr_types.get(source.id as usize) else { continue };
                if !holds_gc(&ck.tcx, t, 0, 0) {
                    continue;
                }
                errors.push((
                    source.span,
                    format!(
                        "'{}' cannot be sent to another thread: a collector object could be changed by two threads at once",
                        ck.tcx.name_of(t)
                    ),
                ));
            }
        });
    }
    errors.sort_by_key(|(s, _)| (s.file, s.line, s.col));
    errors.dedup_by(|a, b| a.0.line == b.0.line && a.0.col == b.0.col);
    for (span, msg) in errors {
        ck.dg.error(span, msg);
    }
}
