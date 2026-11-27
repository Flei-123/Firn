// SPDX-License-Identifier: MPL-2.0
use crate::method_modules::METHOD_MODULES;
use crate::prelude_format;
use std::sync::atomic::{AtomicBool, Ordering};

static DISABLED: AtomicBool = AtomicBool::new(false);

const KERNEL_MARK: &str = "profile kernel";
const MODULE_NAMES: [(&str, &str); 3] = [("parse", "std.parse"), ("format", "std.format"), ("chars", "std.chars")];
const WRAPPERS: [(&str, &str); 3] = [
    ("println", "fn println(s: str) { io.print_line(s) }"),
    ("print", "fn print(s: str) { io.print(s) }"),
    ("readln", "fn readln() -> str { return io.read_line() }"),
];

pub(crate) fn is_disabled() -> bool {
    DISABLED.load(Ordering::Relaxed)
}

pub fn disable() {
    DISABLED.store(true, Ordering::Relaxed);
}

#[derive(Default)]
struct Usage {
    dotted: usize,
    called: usize,
    other: usize,
    declared: usize,
}

#[derive(Default)]
struct Scan {
    names: std::collections::HashMap<String, Usage>,
    imported: Vec<String>,
    to_str_receivers: Vec<String>,
    declares_to_str_method: bool,
    format_calls: Vec<usize>,
}

pub(crate) fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

pub(crate) fn skip_line(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i] != b'\n' {
        i += 1;
    }
    i
}

pub(crate) fn skip_block(b: &[u8], mut i: usize) -> usize {
    while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
        i += 1;
    }
    (i + 2).min(b.len())
}

pub(crate) fn skip_quoted(b: &[u8], mut i: usize, quote: u8) -> usize {
    i += 1;
    while i < b.len() && b[i] != quote && b[i] != b'\n' {
        if b[i] == b'\\' {
            i += 1;
        }
        i += 1;
    }
    (i + 1).min(b.len())
}

pub(crate) fn next_non_space(b: &[u8], mut i: usize) -> Option<u8> {
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    b.get(i).copied()
}

pub(crate) fn read_import_path(b: &[u8], mut i: usize) -> (Option<String>, usize) {
    let mut last = None;
    loop {
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        let start = i;
        while i < b.len() && is_ident_byte(b[i]) {
            i += 1;
        }
        if start == i {
            break;
        }
        last = Some(String::from_utf8_lossy(&b[start..i]).to_string());
        if b.get(i) != Some(&b'.') {
            break;
        }
        i += 1;
    }
    (last, i)
}

fn receiver_before(src: &str, start: usize) -> String {
    let b = src.as_bytes();
    let mut j = start;
    while j > 0 && b[j - 1].is_ascii_whitespace() {
        j -= 1;
    }
    j = j.saturating_sub(1);
    let end = j;
    while j > 0 && is_ident_byte(b[j - 1]) {
        j -= 1;
    }
    src[j..end].to_string()
}

impl Scan {
    fn calls_to_str_method(&self) -> bool {
        self.to_str_receivers.iter().any(|r| r.is_empty() || !self.imported.contains(r))
    }
}

fn is_method_declaration(b: &[u8], mut i: usize) -> bool {
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    if b.get(i) != Some(&b'(') {
        return false;
    }
    i += 1;
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    b.get(i) == Some(&b'*') || b[i..].starts_with(b"self")
}

fn note_ident(scan: &mut Scan, name: &str, prev_dot: bool, prev_word: &str, next: Option<u8>) {
    if name == "to_str" {
        return;
    }
    if prev_dot {
        return;
    }
    let usage = scan.names.entry(name.to_string()).or_default();
    if prev_word == "fn" {
        usage.declared += 1;
    } else if next == Some(b'.') {
        usage.dotted += 1;
    } else if next == Some(b'(') {
        usage.called += 1;
    } else {
        usage.other += 1;
    }
}

fn scan_source(src: &str) -> Scan {
    let b = src.as_bytes();
    let mut scan = Scan::default();
    let mut prev_word = String::new();
    let mut prev_dot = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            i = skip_line(b, i);
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = skip_block(b, i + 2);
        } else if c == b'"' || c == b'\'' {
            i = skip_quoted(b, i, c);
            prev_dot = false;
            prev_word.clear();
        } else if c == b'.' {
            prev_dot = true;
            i += 1;
        } else if is_ident_byte(c) && !c.is_ascii_digit() {
            let start = i;
            while i < b.len() && is_ident_byte(b[i]) {
                i += 1;
            }
            let name = &src[start..i];
            if name == "import" {
                let (last, end) = read_import_path(b, i);
                if let Some(l) = last {
                    scan.imported.push(l);
                }
                i = end;
                prev_word.clear();
            } else {
                if name == "to_str" && prev_dot {
                    scan.to_str_receivers.push(receiver_before(src, start));
                }
                if name == "to_str" && prev_word == "fn" && is_method_declaration(b, i) {
                    scan.declares_to_str_method = true;
                }
                if name == "format" && !prev_dot && prev_word != "fn" && next_non_space(b, i) == Some(b'(') {
                    scan.format_calls.push(start);
                }
                note_ident(&mut scan, name, prev_dot, &prev_word, next_non_space(b, i));
                prev_word = name.to_string();
            }
            prev_dot = false;
        } else {
            if !c.is_ascii_whitespace() {
                prev_dot = false;
            }
            i += 1;
        }
    }
    scan
}

fn bare_only_dotted(usage: &Usage) -> bool {
    usage.dotted > 0 && usage.called == 0 && usage.other == 0 && usage.declared == 0
}

fn format_usable(usage: &Usage) -> bool {
    usage.dotted + usage.called > 0 && usage.other == 0 && usage.declared == 0
}

fn bare_only_called(usage: &Usage) -> bool {
    usage.called > 0 && usage.dotted == 0 && usage.other == 0 && usage.declared == 0
}

fn quick_reject(src: &str) -> bool {
    !(src.contains("to_str") || src.contains("parse") || src.contains("format") || src.contains("chars") || src.contains("print") || src.contains("readln"))
}

pub fn expand(src: &str) -> Option<String> {
    if DISABLED.load(Ordering::Relaxed) || quick_reject(src) || src.contains(KERNEL_MARK) {
        return None;
    }
    let scan = scan_source(src);
    let has = |m: &str| scan.imported.iter().any(|x| x == m);
    let mut imports: Vec<&str> = Vec::new();
    let mut wrappers: Vec<&str> = Vec::new();
    let format_ok = scan.names.get("format").is_some_and(format_usable);
    for (name, path) in MODULE_NAMES {
        let module = path.rsplit('.').next().unwrap_or(name);
        let usable = if name == "format" { format_ok } else { scan.names.get(name).is_some_and(bare_only_dotted) };
        if usable && !has(module) {
            imports.push(path);
        }
    }
    let rewrite_calls = format_ok && !scan.format_calls.is_empty();
    if rewrite_calls {
        wrappers.push(prelude_format::SHOW_WRAPPER);
    }
    for (name, text) in WRAPPERS {
        if scan.names.get(name).is_some_and(bare_only_called) {
            wrappers.push(text);
        }
    }
    if !wrappers.is_empty() && !has("io") {
        imports.push("std.io");
    }
    if imports.is_empty() && wrappers.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(src.len() + 128);
    for p in imports {
        out.push_str("import ");
        out.push_str(p);
        out.push_str("; ");
    }
    if rewrite_calls {
        out.push_str(&prelude_format::rewrite(src, &scan.format_calls));
    } else {
        out.push_str(src);
    }
    if !wrappers.is_empty() {
        out.push('\n');
    }
    for w in wrappers {
        out.push_str(w);
        out.push('\n');
    }
    Some(out)
}

pub fn needs_tostr<'a>(sources: impl Iterator<Item = &'a str>) -> bool {
    if DISABLED.load(Ordering::Relaxed) {
        return false;
    }
    let mut wanted = false;
    for src in sources {
        let aliased = crate::csalias::expand(src);
        let src = aliased.as_deref().unwrap_or(src);
        if src.contains(KERNEL_MARK) || !src.contains("to_str") {
            continue;
        }
        let scan = scan_source(src);
        if scan.declares_to_str_method || scan.imported.iter().any(|x| x == "tostr") {
            return false;
        }
        wanted |= scan.calls_to_str_method();
    }
    wanted
}

pub fn needed_method_modules<'a>(sources: impl Iterator<Item = &'a str>) -> Vec<&'static str> {
    let mut wanted = vec![false; METHOD_MODULES.len()];
    if DISABLED.load(Ordering::Relaxed) {
        return Vec::new();
    }
    for src in sources {
        if src.contains(KERNEL_MARK) {
            continue;
        }
        let aliased = crate::csalias::expand(src);
        mark_method_calls(aliased.as_deref().unwrap_or(src).as_bytes(), &mut wanted);
    }
    METHOD_MODULES.iter().zip(wanted).filter(|(_, w)| *w).map(|((m, _), _)| *m).collect()
}

fn mark_method_calls(b: &[u8], wanted: &mut [bool]) {
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'.' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut end = start;
        while end < b.len() && is_ident_byte(b[end]) {
            end += 1;
        }
        if end > start && b.get(end) == Some(&b'(') {
            let name = &b[start..end];
            for (k, (_, names)) in METHOD_MODULES.iter().enumerate() {
                if names.iter().any(|n| n.as_bytes() == name) {
                    wanted[k] = true;
                }
            }
        }
        i = end.max(i + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn added(src: &str) -> String {
        expand(src).map(|t| t.replace(src, "")).unwrap_or_default()
    }

    #[test]
    fn method_calls_select_modules() {
        assert_eq!(needed_method_modules(["fn f() { let a = x.to_upper() }"].into_iter()), vec!["strmethods"]);
        assert_eq!(needed_method_modules(["fn f() { let a = x.sqrt() + s.trim() }"].into_iter()), vec!["nummethods", "strmethods"]);
        assert!(needed_method_modules(["fn f() { let a = sqrt(x) }"].into_iter()).is_empty());
        assert!(needed_method_modules(["profile kernel\nfn f() { x.sqrt() }"].into_iter()).is_empty());
    }

    #[test]
    fn to_str_needs_a_use_and_no_own_declaration() {
        assert!(needs_tostr(["fn main() -> i32 { let s = a.to_str()\n return 0 }"].into_iter()));
        assert!(!needs_tostr(["fn main() -> i32 { return 0 }"].into_iter()));
        assert!(!needs_tostr(["import std.chars\nfn main() -> i32 { let s = chars.to_str(65)\n return 0 }"].into_iter()));
        assert!(needs_tostr(["fn main() -> i32 { let s = f(1).to_str()\n return 0 }"].into_iter()));
        assert!(!needs_tostr(["fn main() -> i32 { let s = a.to_str()\n return 0 }", "impl i32 { fn to_str(*self) -> str { return \"\" } }"].into_iter()));
        assert!(!needs_tostr(["import std.tostr\nfn main() -> i32 { let s = a.to_str()\n return 0 }"].into_iter()));
    }

    #[test]
    fn module_names_need_pure_dotted_use() {
        assert_eq!(added("fn main() -> i32 { return parse.to_i32(s) catch 0 }"), "import std.parse; ");
        assert_eq!(added("fn main() -> i32 { let parse = 1\n return parse.x }"), "");
        assert_eq!(added("import std.parse\nfn main() -> i32 { return parse.to_i32(s) catch 0 }"), "");
    }

    #[test]
    fn print_wrappers() {
        let a = added("fn main() -> i32 { println(\"x\")\n return 0 }");
        assert!(a.contains("import std.io; ") && a.contains("fn println(s: str)"));
        assert_eq!(added("fn println(s: str) {}\nfn main() -> i32 { println(\"x\")\n return 0 }"), "");
        assert_eq!(added("import std.io\nfn main() -> i32 { io.print(\"x\")\n return 0 }"), "");
    }

    #[test]
    fn kernel_untouched() {
        assert_eq!(added("profile kernel\nfn main() -> i32 { println(\"x\")\n return 0 }"), "");
    }
}
