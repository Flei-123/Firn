use crate::ast::{ExprKind, Program};
use crate::diag::Span;
use crate::gcfree;
use crate::refparam::walk_exprs;
use crate::sema::Checker;
use std::collections::HashMap;

const STR_COPY: &str = "__str_copy";

const HELP: &str = "build the text in a std.str.Bytes buffer or a Vec[u8]";

fn message() -> String {
    gcfree::message("'str' allocation")
}

fn calls_of(prog: &Program, index: usize) -> Vec<(String, Span)> {
    let mut calls = Vec::new();
    walk_exprs(&prog.funcs[index].body, &mut |e| {
        if let ExprKind::Call(name, _, _) = &e.kind {
            calls.push((name.clone(), e.span));
        }
    });
    calls
}

pub(crate) fn hook_check(ck: &mut Checker, prog: &Program) {
    if !gcfree::enabled() {
        return;
    }
    let Some(main) = prog.funcs.iter().find(|f| f.name == "main") else { return };
    let root_file = main.span.file;
    let mut by_name: HashMap<&str, usize> = HashMap::new();
    for (i, f) in prog.funcs.iter().enumerate() {
        by_name.entry(f.name.as_str()).or_insert(i);
    }
    let mut origin: Vec<Option<Span>> = vec![None; prog.funcs.len()];
    let mut seen = vec![false; prog.funcs.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (i, f) in prog.funcs.iter().enumerate() {
        if f.span.file == root_file {
            seen[i] = true;
            stack.push(i);
        }
    }
    let mut errors: Vec<Span> = Vec::new();
    while let Some(i) = stack.pop() {
        let in_root = prog.funcs[i].span.file == root_file;
        for (name, span) in calls_of(prog, i) {
            let blame = if in_root { span } else { origin[i].unwrap_or(span) };
            if name == STR_COPY {
                if prog.funcs[i].name != STR_COPY {
                    errors.push(blame);
                }
                continue;
            }
            let Some(&j) = by_name.get(name.as_str()) else { continue };
            if !seen[j] {
                seen[j] = true;
                origin[j] = Some(blame);
                stack.push(j);
            }
        }
    }
    errors.sort_by_key(|s| (s.file, s.line, s.col));
    errors.dedup_by(|a, b| a.file == b.file && a.line == b.line && a.col == b.col);
    for span in errors {
        ck.dg.error_help(span, message(), HELP);
    }
}
