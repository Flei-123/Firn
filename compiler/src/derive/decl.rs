// SPDX-License-Identifier: MPL-2.0
use super::{fail, Fail};

pub struct Variant {
    pub name: String,
    pub types: Vec<String>,
}

pub enum Body {
    Struct(Vec<(String, String)>),
    Enum(Vec<Variant>),
}

pub struct Decl {
    pub name: String,
    pub generics: Vec<String>,
    pub body: Body,
    pub end: usize,
}

impl Decl {
    pub fn self_type(&self) -> String {
        if self.generics.is_empty() {
            return self.name.clone();
        }
        format!("{}[{}]", self.name, self.generics.join(", "))
    }
}

fn skip_blank(src: &str, mut at: usize) -> usize {
    let b = src.as_bytes();
    loop {
        while at < b.len() && b[at].is_ascii_whitespace() {
            at += 1;
        }
        if src[at..].starts_with("//") {
            at += src[at..].find('\n').unwrap_or(src.len() - at);
        } else if src[at..].starts_with("#[") {
            at += src[at..].find(']').map_or(src.len() - at, |i| i + 1);
        } else {
            return at;
        }
    }
}

fn word(src: &str, at: usize) -> &str {
    let rest = &src[at..];
    let n = rest.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(rest.len());
    &rest[..n]
}

fn matching(src: &str, open: usize, close: char) -> Option<usize> {
    let b = src.as_bytes();
    let mut depth = 0;
    let mut at = open;
    while at < b.len() {
        if src[at..].starts_with("//") {
            at += src[at..].find('\n').unwrap_or(src.len() - at);
            continue;
        }
        match b[at] {
            b'{' | b'[' | b'(' => depth += 1,
            b'}' | b']' | b')' => {
                depth -= 1;
                if depth == 0 {
                    return if b[at] as char == close { Some(at) } else { None };
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn strip_comments(text: &str) -> String {
    text.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

pub fn split_top(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && (c == ',' || c == '\n') {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn fields(src: &str, at: usize, body: &str) -> Result<Vec<(String, String)>, Fail> {
    let mut out = Vec::new();
    for entry in split_top(&strip_comments(body)) {
        let Some((name, ty)) = entry.split_once(':') else {
            return Err(fail(src, at, 1, format!("derive: cannot read the struct field '{}'", entry)));
        };
        out.push((name.trim().to_string(), ty.trim().to_string()));
    }
    Ok(out)
}

fn variants(src: &str, at: usize, body: &str) -> Result<Vec<Variant>, Fail> {
    let mut out = Vec::new();
    for entry in split_top(&strip_comments(body)) {
        let name = word(&entry, 0).to_string();
        let rest = entry[name.len()..].trim();
        let mut types = Vec::new();
        if let Some(inner) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
            types = split_top(inner);
        } else if !rest.is_empty() || name.is_empty() {
            return Err(fail(src, at, 1, format!("derive: cannot read the enum variant '{}'", entry)));
        }
        out.push(Variant { name, types });
    }
    Ok(out)
}

pub fn parse(src: &str, attr: usize, attr_end: usize) -> Result<Decl, Fail> {
    let mut at = skip_blank(src, attr_end);
    if word(src, at) == "export" {
        at = skip_blank(src, at + "export".len());
    }
    let kind = word(src, at).to_string();
    if kind != "struct" && kind != "enum" {
        return Err(fail(src, attr, 1, "derive attribute must stand before a struct or an enum".to_string()));
    }
    at = skip_blank(src, at + kind.len());
    let name = word(src, at).to_string();
    at = skip_blank(src, at + name.len());
    let mut generics = Vec::new();
    if src[at..].starts_with('[') {
        let Some(close) = matching(src, at, ']') else {
            return Err(fail(src, at, 1, "derive: unterminated type parameter list".to_string()));
        };
        for p in src[at + 1..close].split(',') {
            generics.push(word(p.trim(), 0).to_string());
        }
        at = skip_blank(src, close + 1);
    }
    if !src[at..].starts_with('{') {
        return Err(fail(src, at, 1, format!("derive: expected '{{' after the name of '{}'", name)));
    }
    let Some(close) = matching(src, at, '}') else {
        return Err(fail(src, at, 1, format!("derive: unterminated declaration of '{}'", name)));
    };
    let text = &src[at + 1..close];
    let body = if kind == "struct" {
        Body::Struct(fields(src, at, text)?)
    } else {
        Body::Enum(variants(src, at, text)?)
    };
    Ok(Decl { name, generics, body, end: close + 1 })
}
