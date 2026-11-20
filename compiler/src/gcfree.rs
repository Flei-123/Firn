use crate::diag::Diags;
use crate::lexer::{self, TokKind, Token};
use crate::modules::SourceFile;
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

pub(crate) const FLAG: &str = "--no-gc";

pub(crate) const REQUIRED_OWNERSHIP: &str = "strict";

pub(crate) const RUNTIME_PATH: &str = "lib/gc/nogc.fi";

const RUNTIME: &str = include_str!("../../lib/gc/nogc.fi");

const GC_TYPES: &[&str] = &["Gc", "GcWeak"];

const GC_PREFIXES: &[&str] = &["GcVec", "GcMap", "gcvec_", "gcmap_"];

pub(crate) fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub(crate) fn runtime_source(with_error_set: bool) -> String {
    let mut s = String::new();
    if with_error_set {
        s.push_str("error AllocError { OutOfMemory }\n");
    }
    s.push_str(RUNTIME);
    s
}

pub(crate) fn message(what: &str) -> String {
    format!("{} needs the garbage collector, which '{}' leaves out", what, FLAG)
}

pub(crate) fn help() -> String {
    "own heap values with Vec, Box, Rc or std.str.Bytes (ownership-only mode)".to_string()
}

fn forbidden(toks: &[Token], i: usize) -> Option<&'static str> {
    let next = toks.get(i + 1).map(|t| &t.kind);
    match &toks[i].kind {
        TokKind::Ident(a) if a == "gc" && matches!(next, Some(TokKind::Ident(b)) if b == "class") => {
            Some("'gc class'")
        }
        TokKind::Ident(a) if a == "gc" && matches!(next, Some(TokKind::KwFn)) => Some("'gc fn'"),
        TokKind::Ident(a) if GC_TYPES.contains(&a.as_str()) && matches!(next, Some(TokKind::LBracket)) => {
            Some(if a == "Gc" { "'Gc[T]'" } else { "'GcWeak[T]'" })
        }
        TokKind::Ident(a) if GC_PREFIXES.iter().any(|p| a.starts_with(p)) => Some("'GcVec/GcMap'"),
        _ => None,
    }
}

pub(crate) fn scan(files: &[SourceFile]) -> Result<(), String> {
    for f in files {
        let mut dg = Diags::new(&f.path.display().to_string(), &f.src);
        let toks = lexer::lex_file_cached(&f.src, f.id, &mut dg);
        for i in 0..toks.len() {
            if let Some(what) = forbidden(&toks, i) {
                dg.error_help(toks[i].span, message(what), help());
                return Err(dg.render());
            }
        }
    }
    Ok(())
}

pub(crate) fn ownership_error() -> String {
    format!("'{}' requires --ownership={}", FLAG, REQUIRED_OWNERSHIP)
}
