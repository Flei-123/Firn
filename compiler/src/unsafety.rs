use crate::diag::Span;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    Off,
    Warn,
    Error,
}

pub(crate) const ALLOWED: &str = "off, warn, error";

static MODE: AtomicU8 = AtomicU8::new(0);

static SCOPE: Mutex<Vec<String>> = Mutex::new(Vec::new());

static MARKED: Mutex<Option<HashSet<(u32, u32, u32)>>> = Mutex::new(None);

pub(crate) fn parse(v: &str) -> Option<Mode> {
    match v {
        "off" => Some(Mode::Off),
        "warn" => Some(Mode::Warn),
        "error" => Some(Mode::Error),
        _ => None,
    }
}

pub(crate) fn set_mode(m: Mode) {
    let v = match m {
        Mode::Off => 0,
        Mode::Warn => 1,
        Mode::Error => 2,
    };
    MODE.store(v, Ordering::Relaxed);
}

pub(crate) fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 => Mode::Warn,
        2 => Mode::Error,
        _ => Mode::Off,
    }
}

fn key(s: Span) -> (u32, u32, u32) {
    (s.file, s.line, s.col)
}

pub(crate) fn mark(block_span: Span) {
    let mut g = MARKED.lock().unwrap_or_else(|e| e.into_inner());
    g.get_or_insert_with(HashSet::new).insert(key(block_span));
}

pub(crate) fn is_marked(block_span: Span) -> bool {
    let g = MARKED.lock().unwrap_or_else(|e| e.into_inner());
    g.as_ref().is_some_and(|s| s.contains(&key(block_span)))
}

pub(crate) fn site_message(what: &str) -> String {
    format!("{} is only allowed inside an 'unsafe' block or 'unsafe fn'", what)
}

pub(crate) fn add_scope(prefix: &str) {
    let mut g = SCOPE.lock().unwrap_or_else(|e| e.into_inner());
    g.push(prefix.to_string());
}

pub(crate) fn in_scope(file: &str) -> bool {
    let g = SCOPE.lock().unwrap_or_else(|e| e.into_inner());
    g.is_empty() || g.iter().any(|p| file.contains(p.as_str()))
}
