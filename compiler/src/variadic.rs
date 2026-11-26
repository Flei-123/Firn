// SPDX-License-Identifier: MPL-2.0
use crate::prelude::{is_ident_byte, next_non_space, read_import_path, skip_block, skip_line, skip_quoted};
use crate::prelude_format::split_args;

const KERNEL_MARK: &str = "profile kernel";
const IMPORT: &str = "import std.vec; ";

struct Decl {
    name: String,
    fixed: usize,
    elem: String,
}

fn skip_space(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn read_ident(b: &[u8], i: usize) -> (usize, usize) {
    let mut j = i;
    while j < b.len() && is_ident_byte(b[j]) {
        j += 1;
    }
    (i, j)
}

fn variadic_type(b: &[u8], start: usize, end: usize) -> Option<(usize, usize)> {
    let (n0, n1) = read_ident(b, skip_space(b, start));
    if n0 == n1 {
        return None;
    }
    let colon = skip_space(b, n1);
    if b.get(colon) != Some(&b':') {
        return None;
    }
    let dots = skip_space(b, colon + 1);
    if !b[dots..end].starts_with(b"..") {
        return None;
    }
    let ty = skip_space(b, dots + 2);
    let mut ty_end = end;
    while ty_end > ty && b[ty_end - 1].is_ascii_whitespace() {
        ty_end -= 1;
    }
    (ty < ty_end).then_some((dots, ty_end))
}

fn is_method(b: &[u8], first: (usize, usize)) -> bool {
    let text = &b[first.0..first.1];
    let t = std::str::from_utf8(text).unwrap_or("").trim();
    t == "self" || t.starts_with("*self") || t.starts_with("*mut self") || t.starts_with("self:")
}

fn declarations(src: &str) -> (Vec<(usize, usize, String)>, Vec<Decl>, Vec<String>) {
    let b = src.as_bytes();
    let mut edits = Vec::new();
    let mut decls = Vec::new();
    let mut imported = Vec::new();
    let mut prev_word = String::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            i = skip_line(b, i);
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = skip_block(b, i + 2);
        } else if c == b'"' || c == b'\'' {
            i = skip_quoted(b, i, c);
            prev_word.clear();
        } else if is_ident_byte(c) && !c.is_ascii_digit() {
            let (start, end) = read_ident(b, i);
            i = end;
            let word = &src[start..end];
            if word == "import" {
                let (last, stop) = read_import_path(b, i);
                imported.extend(last);
                i = stop;
                prev_word.clear();
                continue;
            }
            if prev_word == "fn" {
                let open = skip_space(b, end);
                if b.get(open) == Some(&b'(') {
                    declaration(src, word, open, &mut edits, &mut decls);
                }
            }
            prev_word = word.to_string();
        } else {
            if !c.is_ascii_whitespace() {
                prev_word.clear();
            }
            i += 1;
        }
    }
    (edits, decls, imported)
}

fn declaration(src: &str, name: &str, open: usize, edits: &mut Vec<(usize, usize, String)>, decls: &mut Vec<Decl>) {
    let b = src.as_bytes();
    let Some(call) = split_args(b, open) else { return };
    let Some(&(s, e)) = call.args.last() else { return };
    let Some((dots, ty_end)) = variadic_type(b, s, e) else { return };
    let elem = src[skip_space(b, dots + 2)..ty_end].to_string();
    edits.push((dots, ty_end, format!("Vec[{}]", elem)));
    if is_method(b, call.args[0]) {
        return;
    }
    decls.push(Decl { name: name.to_string(), fixed: call.args.len() - 1, elem });
}

fn apply(src: &str, edits: &[(usize, usize, String)]) -> String {
    let mut out = String::with_capacity(src.len() + edits.len() * 16);
    let mut pos = 0;
    for (s, e, text) in edits {
        out.push_str(&src[pos..*s]);
        out.push_str(text);
        pos = *e;
    }
    out.push_str(&src[pos..]);
    out
}

fn interpolated_sites(src: &str, quote: usize, decls: &[Decl], sites: &mut Vec<(usize, usize)>) -> usize {
    let b = src.as_bytes();
    let mut depth = 0usize;
    let mut i = quote + 1;
    while i < b.len() && b[i] != b'\n' && !(b[i] == b'"' && depth == 0) {
        match b[i] {
            b'\\' => i += 1,
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            c if depth > 0 && is_ident_byte(c) && !c.is_ascii_digit() => {
                let (start, end) = read_ident(b, i);
                let after_dot = start > 0 && (b[start - 1] == b'.' || is_ident_byte(b[start - 1]));
                let k = decls.iter().position(|d| d.name == src[start..end]);
                if let Some(k) = k.filter(|_| !after_dot && next_non_space(b, end) == Some(b'(')) {
                    sites.push((start, k));
                }
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    (i + 1).min(b.len())
}

fn call_sites(src: &str, decls: &[Decl]) -> Vec<(usize, usize)> {
    let b = src.as_bytes();
    let mut sites = Vec::new();
    let mut prev_word = String::new();
    let mut prev_dot = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            i = skip_line(b, i);
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = skip_block(b, i + 2);
        } else if c == b'"' && i > 0 && b[i - 1] == b'f' && (i < 2 || !is_ident_byte(b[i - 2])) {
            i = interpolated_sites(src, i, decls, &mut sites);
            prev_dot = false;
            prev_word.clear();
        } else if c == b'"' || c == b'\'' {
            i = skip_quoted(b, i, c);
            prev_dot = false;
            prev_word.clear();
        } else if c == b'.' {
            prev_dot = true;
            i += 1;
        } else if is_ident_byte(c) && !c.is_ascii_digit() {
            let (start, end) = read_ident(b, i);
            i = end;
            let word = &src[start..end];
            let found = decls.iter().position(|d| d.name == word);
            if let Some(k) = found.filter(|_| !prev_dot && prev_word != "fn" && next_non_space(b, end) == Some(b'(')) {
                sites.push((start, k));
            }
            prev_word = word.to_string();
            prev_dot = false;
        } else {
            if !c.is_ascii_whitespace() {
                prev_dot = false;
            }
            i += 1;
        }
    }
    sites
}

fn pack(elem: &str, extra: &[String]) -> String {
    let mut out = format!("vec.vec_new[{}]()", elem);
    for arg in extra {
        out = format!("vec.vec_with[{}]({}, {})", elem, out, arg);
    }
    out
}

fn emit(src: &str, from: usize, to: usize, sites: &[(usize, usize)], decls: &[Decl], out: &mut String) {
    let b = src.as_bytes();
    let mut pos = from;
    for &(p, k) in sites {
        if p < pos {
            continue;
        }
        if p >= to {
            break;
        }
        let decl = &decls[k];
        let Some(call) = split_args(b, skip_space(b, p + decl.name.len())).filter(|c| c.close < to) else {
            continue;
        };
        let given = if call.args.len() == 1 && call.args[0].0 == call.args[0].1 { 0 } else { call.args.len() };
        if given < decl.fixed {
            continue;
        }
        out.push_str(&src[pos..p]);
        out.push_str(&decl.name);
        out.push('(');
        let mut parts = Vec::new();
        for &(s, e) in call.args.iter().take(given) {
            let mut text = String::new();
            emit(src, s, e, sites, decls, &mut text);
            parts.push(text);
        }
        let extra = parts.split_off(decl.fixed.min(parts.len()));
        for part in &parts {
            out.push_str(part);
            out.push(',');
        }
        out.push_str(&pack(&decl.elem, &extra));
        out.push(')');
        pos = call.close + 1;
    }
    out.push_str(&src[pos..to]);
}

pub fn expand(src: &str) -> Option<String> {
    if !src.contains("..") || src.contains(KERNEL_MARK) {
        return None;
    }
    let (edits, decls, imported) = declarations(src);
    if edits.is_empty() {
        return None;
    }
    let declared = apply(src, &edits);
    let sites = call_sites(&declared, &decls);
    let mut body = String::with_capacity(declared.len() + sites.len() * 48);
    emit(&declared, 0, declared.len(), &sites, &decls, &mut body);
    if imported.iter().any(|m| m == "vec") {
        return Some(body);
    }
    Some(format!("{}{}", IMPORT, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_and_calls_are_rewritten() {
        let src = "import std.vec\nfn sum(k: i32, xs: ..i32) -> i32 { return k }\nfn main() -> i32 { return sum(1, 2, 3) + sum(0) }";
        let out = expand(src).unwrap();
        assert!(out.contains("fn sum(k: i32, xs: Vec[i32])"));
        assert!(out.contains("sum(1,vec.vec_with[i32](vec.vec_with[i32](vec.vec_new[i32](),  2),  3))"));
        assert!(out.contains("sum(0,vec.vec_new[i32]())"));
    }

    #[test]
    fn plain_ranges_and_kernel_are_untouched() {
        assert!(expand("fn main() -> i32 { for i in 0..4 { } return 0 }").is_none());
        assert!(expand("profile kernel\nfn f(xs: ..i32) {}").is_none());
    }
}
