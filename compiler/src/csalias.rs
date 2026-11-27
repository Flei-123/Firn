// SPDX-License-Identifier: MPL-2.0
use crate::csalias_table::ALIASES;
use crate::prelude::{is_disabled, is_ident_byte, next_non_space, skip_block, skip_line, skip_quoted};

const KERNEL_MARK: &str = "profile kernel";
const FLAG_BARE: u8 = 1;
const FLAG_PROP: u8 = 2;

#[derive(Clone, Copy, PartialEq)]
enum Form {
    DottedCall,
    BareCall,
    Property,
}

struct Site {
    start: usize,
    end: usize,
    alias: usize,
    form: Form,
}

#[derive(Default)]
struct Scan {
    sites: Vec<Site>,
    owned: Vec<bool>,
    user_import: bool,
}

pub(crate) fn alias_index(name: &str) -> Option<usize> {
    ALIASES.iter().position(|a| a.0 == name)
}

fn is_ident_start(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphabetic()
}

fn is_fstring_quote(b: &[u8], i: usize) -> bool {
    i >= 1 && b[i - 1] == b'f' && (i < 2 || !is_ident_byte(b[i - 2]))
}

struct Walk {
    prev_dot: bool,
    prev_fn: bool,
    last_ident: bool,
    last_type_like: bool,
    receiver_is_type: bool,
}

impl Walk {
    fn new() -> Walk {
        Walk { prev_dot: false, prev_fn: false, last_ident: false, last_type_like: false, receiver_is_type: false }
    }

    fn reset_after_literal(&mut self) {
        self.prev_dot = false;
        self.prev_fn = false;
        self.last_ident = false;
    }
}

fn note_identifier(scan: &mut Scan, w: &Walk, b: &[u8], start: usize, end: usize, next: Option<u8>) {
    let name = &b[start..end];
    if !name[0].is_ascii_uppercase() {
        return;
    }
    let Some(alias) = std::str::from_utf8(name).ok().and_then(alias_index) else {
        return;
    };
    let flags = ALIASES[alias].3;
    let form = if w.prev_dot && !w.receiver_is_type && next == Some(b'(') {
        Some(Form::DottedCall)
    } else if w.prev_dot && !w.receiver_is_type && flags & FLAG_PROP != 0 {
        Some(Form::Property)
    } else if !w.prev_dot && !w.prev_fn && flags & FLAG_BARE != 0 && next == Some(b'(') {
        Some(Form::BareCall)
    } else {
        None
    };
    match form {
        Some(form) => scan.sites.push(Site { start, end, alias, form }),
        None => scan.owned[alias] = true,
    }
}

fn note_import(scan: &mut Scan, b: &[u8], mut i: usize) {
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    let start = i;
    while i < b.len() && is_ident_byte(b[i]) {
        i += 1;
    }
    if i > start && &b[start..i] != b"std" {
        scan.user_import = true;
    }
}

fn scan_interpolation(scan: &mut Scan, b: &[u8], quote: usize, end: usize) -> usize {
    let mut j = quote + 1;
    while j < end && b[j] != b'"' && b[j] != b'\n' {
        if b[j] == b'\\' {
            j += 2;
        } else if b[j] == b'{' && b.get(j + 1) == Some(&b'{') {
            j += 2;
        } else if b[j] == b'{' {
            let close = expression_end(b, j, end);
            let stop = if close > j + 2 { close - 1 } else { j + 1 };
            scan_range(scan, b, j + 1, spec_start(b, j + 1, stop));
            j = close;
        } else {
            j += 1;
        }
    }
    (j + 1).min(end)
}

fn expression_end(b: &[u8], open: usize, end: usize) -> usize {
    let mut depth = 1;
    let mut k = open + 1;
    while k < end && depth > 0 {
        if b[k] == b'"' {
            k = skip_quoted(b, k, b'"');
        } else {
            if b[k] == b'{' {
                depth += 1;
            }
            if b[k] == b'}' {
                depth -= 1;
            }
            k += 1;
        }
    }
    k
}

fn spec_start(b: &[u8], from: usize, to: usize) -> usize {
    let mut i = from;
    while i < to {
        if b[i] == b':' {
            if i + 1 < to && b[i + 1] == b':' {
                i += 2;
            } else {
                return i;
            }
        } else {
            i += 1;
        }
    }
    to
}

fn scan_range(scan: &mut Scan, b: &[u8], from: usize, end: usize) {
    let mut w = Walk::new();
    let mut i = from;
    while i < end {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            i = skip_line(b, i);
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = skip_block(b, i + 2);
        } else if c == b'"' && is_fstring_quote(b, i) {
            i = scan_interpolation(scan, b, i, end);
            w.reset_after_literal();
        } else if c == b'"' || c == b'\'' {
            i = skip_quoted(b, i, c);
            w.reset_after_literal();
        } else if c == b'.' {
            w.receiver_is_type = w.last_ident && w.last_type_like;
            w.prev_dot = true;
            w.last_ident = false;
            i += 1;
        } else if is_ident_start(c) {
            let start = i;
            while i < end && is_ident_byte(b[i]) {
                i += 1;
            }
            if &b[start..i] == b"import" {
                note_import(scan, b, i);
            }
            note_identifier(scan, &w, b, start, i, next_non_space(b, i));
            w.prev_fn = &b[start..i] == b"fn";
            w.last_type_like = b[start].is_ascii_uppercase() && !w.prev_dot;
            w.prev_dot = false;
            w.last_ident = true;
        } else {
            if c > b' ' {
                w.prev_dot = false;
                w.last_ident = false;
            }
            i += 1;
        }
    }
}

fn replacement(site: &Site) -> String {
    let (_, target, prefix, _) = ALIASES[site.alias];
    let mut text = match site.form {
        Form::BareCall => format!("{prefix}{target}"),
        Form::DottedCall => target.to_string(),
        Form::Property => format!("{target}()"),
    };
    let width = site.end - site.start;
    while text.len() < width {
        text.push(' ');
    }
    text
}

pub fn expand(src: &str) -> Option<String> {
    if is_disabled() || src.contains(KERNEL_MARK) {
        return None;
    }
    let b = src.as_bytes();
    let mut scan = Scan { sites: Vec::new(), owned: vec![false; ALIASES.len()], user_import: false };
    scan_range(&mut scan, b, 0, b.len());
    if scan.user_import {
        return None;
    }
    let sites: Vec<&Site> = scan.sites.iter().filter(|s| !scan.owned[s.alias]).collect();
    if sites.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(src.len() + 16);
    let mut last = 0;
    for site in sites {
        out.push_str(&src[last..site.start]);
        out.push_str(&replacement(site));
        last = site.end;
    }
    out.push_str(&src[last..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> String {
        expand(src).unwrap_or_else(|| src.to_string())
    }

    #[test]
    fn dotted_calls_are_renamed_and_padded() {
        assert_eq!(run("let s = x.ToString()"), "let s = x.to_str  ()");
        assert_eq!(run("a.Add(1)\nb.Trim()"), "a.push(1)\nb.trim()");
    }

    #[test]
    fn bare_calls_use_prefix_only_when_listed() {
        assert_eq!(run("Println(\"x\")"), "println(\"x\")");
        assert_eq!(run("let l = ReadLine()"), "let l = io.read_line()");
        assert_eq!(run("Sleep(10)"), "time.sleep_ms(10)");
        assert_eq!(run("Add(1, 2)"), "Add(1, 2)");
    }

    #[test]
    fn properties() {
        assert_eq!(run("let n = v.Count"), "let n = v.len()");
        assert_eq!(run("let n = s.Length"), "let n = s.length()");
    }

    #[test]
    fn own_definitions_win() {
        let own = "fn Add(a: i32, b: i32) -> i32 { return a + b }\nfn main() { let x = c.Add(1, 2) }";
        assert_eq!(run(own), own);
        let field = "struct P { Count: i32 }\nfn main() { let n = p.Count }";
        assert_eq!(run(field), field);
        let variant = "enum Op { Add(i32) }\nfn main() { let o = Op.Add(1) }";
        assert_eq!(run(variant), variant);
    }

    #[test]
    fn type_paths_are_left_alone() {
        assert_eq!(run("let o = Op.Add(1)"), "let o = Op.Add(1)");
        assert_eq!(run("let n = p.Name.ToUpper()"), "let n = p.Name.to_upper()");
    }

    #[test]
    fn fstring_interpolation_and_comments() {
        assert_eq!(run("println(f\"{x.ToString()}\")"), "println(f\"{x.to_str  ()}\")");
        assert_eq!(run("// x.ToString()\n\"x.ToString()\""), "// x.ToString()\n\"x.ToString()\"");
    }

    #[test]
    fn files_importing_user_modules_are_untouched() {
        let own = "import calc\nfn main() { let a = calc.Add(1, 2)\nlet b = x.ToString() }";
        assert_eq!(run(own), own);
        let std_only = "import std.io\nfn main() { let b = x.ToString() }";
        assert_eq!(run(std_only), "import std.io\nfn main() { let b = x.to_str  () }");
    }

    #[test]
    fn kernel_untouched() {
        let k = "profile kernel\nx.ToString()";
        assert_eq!(run(k), k);
    }
}
