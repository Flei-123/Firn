// SPDX-License-Identifier: GPL-2.0-only
//! Round 64, point 4 — THE LANGUAGE SERVER (`firnc --lsp`).
//!
//! It speaks the Language Server Protocol over standard input/output and sits
//! ON TOP of the compiler that is already there: the same lexer, the same
//! parser, the same type checker. That is the whole point — an editor must
//! not see a second, slightly different Firn. Whatever `firnc` says, the
//! editor says.
//!
//! WITHOUT A FOREIGN LIBRARY. The protocol is JSON over a `Content-Length`
//! header; both are small enough to write out. `Json` below is a reader and
//! a writer for exactly the subset the protocol uses.
//!
//! WHAT IT CAN DO:
//!   * `textDocument/publishDiagnostics` — errors while typing, with the
//!     texts of `firnc` including the suggestions of round 64
//!   * `textDocument/definition` — jump to the declaration
//!   * `textDocument/completion` — names in scope plus the keywords
//!   * `textDocument/rename` — rename, with the scope taken into account
//!   * `textDocument/hover` — what the name under the cursor is
//!   * `textDocument/formatting` — the shape of `firnfmt`, over the same
//!     rules (blanks only)
//!
//! WHAT IT CANNOT DO, AND SAYS SO: it works on ONE file at a time. `import`
//! is not followed, so a name out of another module counts as unknown for
//! „jump to definition" — the diagnostics come from a compilation of the
//! whole module tree all the same, so nothing is claimed that is untrue.

use std::collections::HashMap;
use std::io::{Read, Write};

use crate::ast::{Block, Program, Stmt};
use crate::diag::{Diags, Span};
use crate::lexer::{self, TokKind};

// ------------------------------------------------------------------- JSON

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn path(&self, keys: &[&str]) -> Option<&Json> {
        let mut cur = self;
        for k in keys {
            cur = cur.get(k)?;
        }
        Some(cur)
    }
    pub fn text(&self) -> &str {
        match self {
            Json::Str(s) => s,
            _ => "",
        }
    }
    pub fn number(&self) -> i64 {
        match self {
            Json::Num(n) => *n as i64,
            _ => 0,
        }
    }

    pub fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 9e15 {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{}", n));
                }
            }
            Json::Str(s) => write_text(s, out),
            Json::Arr(a) => {
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            Json::Obj(m) => {
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_text(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }

    pub fn to_text(&self) -> String {
        let mut s = String::new();
        self.write(&mut s);
        s
    }
}

fn write_text(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn blank(&mut self) {
        while self.i < self.b.len() && (self.b[self.i] as char).is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn value(&mut self) -> Option<Json> {
        self.blank();
        let c = *self.b.get(self.i)? as char;
        match c {
            '{' => {
                self.i += 1;
                let mut m = Vec::new();
                loop {
                    self.blank();
                    if *self.b.get(self.i)? as char == '}' {
                        self.i += 1;
                        break;
                    }
                    let k = match self.value()? {
                        Json::Str(s) => s,
                        _ => return None,
                    };
                    self.blank();
                    if *self.b.get(self.i)? as char != ':' {
                        return None;
                    }
                    self.i += 1;
                    let v = self.value()?;
                    m.push((k, v));
                    self.blank();
                    if *self.b.get(self.i)? as char == ',' {
                        self.i += 1;
                    }
                }
                Some(Json::Obj(m))
            }
            '[' => {
                self.i += 1;
                let mut a = Vec::new();
                loop {
                    self.blank();
                    if *self.b.get(self.i)? as char == ']' {
                        self.i += 1;
                        break;
                    }
                    a.push(self.value()?);
                    self.blank();
                    if *self.b.get(self.i)? as char == ',' {
                        self.i += 1;
                    }
                }
                Some(Json::Arr(a))
            }
            '"' => {
                self.i += 1;
                let mut s = String::new();
                loop {
                    let c = *self.b.get(self.i)? as char;
                    self.i += 1;
                    if c == '"' {
                        break;
                    }
                    if c != '\\' {
                        // Multi-octet UTF-8 characters travel through
                        // unchanged; only the leading octet lands here.
                        if (c as u32) < 0x80 {
                            s.push(c);
                        } else {
                            let start = self.i - 1;
                            let mut end = self.i;
                            while end < self.b.len() && self.b[end] & 0xc0 == 0x80 {
                                end += 1;
                            }
                            s.push_str(&String::from_utf8_lossy(&self.b[start..end]));
                            self.i = end;
                        }
                        continue;
                    }
                    let e = *self.b.get(self.i)? as char;
                    self.i += 1;
                    match e {
                        'n' => s.push('\n'),
                        'r' => s.push('\r'),
                        't' => s.push('\t'),
                        'b' => s.push('\u{08}'),
                        'f' => s.push('\u{0c}'),
                        'u' => {
                            let h = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
                            self.i += 4;
                            let v = u32::from_str_radix(h, 16).ok()?;
                            // A surrogate pair is written as two escapes.
                            if (0xd800..0xdc00).contains(&v)
                                && self.b.get(self.i) == Some(&b'\\')
                                && self.b.get(self.i + 1) == Some(&b'u')
                            {
                                let h2 =
                                    std::str::from_utf8(self.b.get(self.i + 2..self.i + 6)?).ok()?;
                                let v2 = u32::from_str_radix(h2, 16).ok()?;
                                if (0xdc00..0xe000).contains(&v2) {
                                    self.i += 6;
                                    let cp = 0x10000 + ((v - 0xd800) << 10) + (v2 - 0xdc00);
                                    s.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                                    continue;
                                }
                            }
                            s.push(char::from_u32(v).unwrap_or('\u{fffd}'));
                        }
                        other => s.push(other),
                    }
                }
                Some(Json::Str(s))
            }
            't' => {
                self.i += 4;
                Some(Json::Bool(true))
            }
            'f' => {
                self.i += 5;
                Some(Json::Bool(false))
            }
            'n' => {
                self.i += 4;
                Some(Json::Null)
            }
            _ => {
                let start = self.i;
                while self.i < self.b.len() {
                    let c = self.b[self.i] as char;
                    if c.is_ascii_digit() || "+-.eE".contains(c) {
                        self.i += 1;
                    } else {
                        break;
                    }
                }
                std::str::from_utf8(&self.b[start..self.i])
                    .ok()?
                    .parse::<f64>()
                    .ok()
                    .map(Json::Num)
            }
        }
    }
}

pub fn parse_json(text: &str) -> Option<Json> {
    Reader { b: text.as_bytes(), i: 0 }.value()
}

fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}
fn text(s: &str) -> Json {
    Json::Str(s.to_string())
}
fn num(n: i64) -> Json {
    Json::Num(n as f64)
}

// ------------------------------------------------------------- the symbols

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    /// `function`, `struct`, `const`, `parameter`, `variable`, `module`
    pub kind: &'static str,
    /// Position of the declaration (1-based, like everywhere in the compiler)
    pub line: u32,
    pub col: u32,
    /// Signature or type as text, for `hover`.
    pub about: String,
    /// `None` = visible in the whole file. `Some((from, to))` = only inside
    /// these lines (a local variable, a parameter).
    pub scope: Option<(u32, u32)>,
}

/// Everything an editor needs about a file.
pub struct Analysis {
    pub text: String,
    pub diagnostics: Vec<(Span, String, Option<String>)>,
    pub symbols: Vec<Symbol>,
}

/// The line range of a function: from its `fn` line to the line before the
/// next declaration. Coarse but right — Firn has no nested functions apart
/// from closures, and those live inside the body.
fn fn_range(prog: &Program, idx: usize, last_line: u32) -> (u32, u32) {
    let from = prog.funcs[idx].span.line;
    let mut to = last_line;
    for (j, g) in prog.funcs.iter().enumerate() {
        if j != idx && g.span.line > from && g.span.line - 1 < to {
            to = g.span.line - 1;
        }
    }
    (from, to)
}

fn walk_block(b: &Block, range: (u32, u32), out: &mut Vec<Symbol>) {
    for s in &b.stmts {
        walk_stmt(s, range, out);
    }
}

fn walk_stmt(s: &Stmt, range: (u32, u32), out: &mut Vec<Symbol>) {
    match s {
        Stmt::Let { name, mutable, span, .. } => out.push(Symbol {
            name: name.clone(),
            kind: "variable",
            line: span.line,
            col: span.col,
            about: format!("{} {}", if *mutable { "var" } else { "let" }, name),
            scope: Some(range),
        }),
        Stmt::For { name, name_span, body, .. } => {
            out.push(Symbol {
                name: name.clone(),
                kind: "variable",
                line: name_span.line,
                col: name_span.col,
                about: format!("for {}", name),
                scope: Some(range),
            });
            walk_block(body, range, out);
        }
        Stmt::If { then, els, .. } => {
            walk_block(then, range, out);
            if let Some(e) = els {
                walk_stmt(e, range, out);
            }
        }
        Stmt::While { body, .. } => walk_block(body, range, out),
        Stmt::Block(b) => walk_block(b, range, out),
        Stmt::Defer(inner, _, _) => walk_stmt(inner, range, out),
        _ => {}
    }
}

/// Compiles the text and gathers diagnostics and symbols.
///
/// The diagnostics come from the FULL run (module resolution, type check) so
/// that the editor sees exactly what `firnc` sees. The symbols come from the
/// syntax tree of this ONE file — anything else would name declarations that
/// are not in this file.
pub fn analyse(path: &str, src: &str) -> Analysis {
    let mut dg = Diags::new(path, src);
    let toks = lexer::lex(src, &mut dg);
    let prog = crate::parser::parse(&toks, &mut dg);
    // The type check only when the syntax is sound: otherwise it would
    // report follow-up errors on a broken tree.
    if !dg.has_errors() {
        crate::sema::check(&prog, &mut dg);
    }
    let last_line = src.lines().count().max(1) as u32;

    let mut symbols: Vec<Symbol> = Vec::new();
    for i in &prog.imports {
        symbols.push(Symbol {
            name: i.alias.clone(),
            kind: "module",
            line: i.span.line,
            col: i.span.col,
            about: format!("import {}", i.path.join(".")),
            scope: None,
        });
    }
    for sd in &prog.structs {
        symbols.push(Symbol {
            name: sd.name.clone(),
            kind: "struct",
            line: sd.span.line,
            col: sd.span.col,
            about: format!(
                "struct {} {{ {} }}",
                sd.name,
                sd.fields
                    .iter()
                    .map(|(n, _, _)| n.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            scope: None,
        });
    }
    for c in &prog.consts {
        symbols.push(Symbol {
            name: c.name.clone(),
            kind: "const",
            line: c.span.line,
            col: c.span.col,
            about: format!("const {}", c.name),
            scope: None,
        });
    }
    for (i, f) in prog.funcs.iter().enumerate() {
        symbols.push(Symbol {
            name: f.name.clone(),
            kind: "function",
            line: f.span.line,
            col: f.span.col,
            about: format!(
                "fn {}({})",
                f.name,
                f.params
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            scope: None,
        });
        let range = fn_range(&prog, i, last_line);
        for p in &f.params {
            symbols.push(Symbol {
                name: p.name.clone(),
                kind: "parameter",
                line: p.span.line,
                col: p.span.col,
                about: format!("parameter {}", p.name),
                scope: Some(range),
            });
        }
        walk_block(&f.body, range, &mut symbols);
    }

    Analysis { text: src.to_string(), diagnostics: dg.items_for_lsp(), symbols }
}

/// The name under the position (0-based, as LSP counts), plus its extent.
/// Returns (name, line, col, length) in the counting of the compiler.
pub fn name_at(src: &str, line0: u32, char0: u32) -> Option<(String, u32, u32, u32)> {
    let mut dg = Diags::new("<lsp>", src);
    let toks = lexer::lex(src, &mut dg);
    let line = line0 + 1;
    let col = char0 + 1;
    for t in &toks {
        if t.span.line != line {
            continue;
        }
        if let TokKind::Ident(n) = &t.kind {
            let len = n.chars().count() as u32;
            // The cursor may sit inside the name or right after it.
            if col >= t.span.col && col <= t.span.col + len {
                return Some((n.clone(), t.span.line, t.span.col, len));
            }
        }
    }
    None
}

/// Is the identifier token `i` preceded by a `.` or a `:`? Then it is a
/// field or a part of a path and NOT the name that is being looked for.
fn is_member(toks: &[lexer::Token], i: usize) -> bool {
    if i == 0 {
        return false;
    }
    matches!(toks[i - 1].kind, TokKind::Dot | TokKind::Colon)
}

/// All places where the name `name` stands as a free name, restricted to
/// `scope` if there is one.
pub fn occurrences(src: &str, name: &str, scope: Option<(u32, u32)>) -> Vec<(u32, u32, u32)> {
    let mut dg = Diags::new("<lsp>", src);
    let toks = lexer::lex(src, &mut dg);
    let mut out = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if let TokKind::Ident(n) = &t.kind {
            if n != name || is_member(&toks, i) {
                continue;
            }
            if let Some((from, to)) = scope {
                if t.span.line < from || t.span.line > to {
                    continue;
                }
            }
            out.push((t.span.line, t.span.col, n.chars().count() as u32));
        }
    }
    out
}

/// The symbol that fits `name` at `line`: a local one of the enclosing
/// function beats a global one.
pub fn resolve<'a>(a: &'a Analysis, name: &str, line: u32) -> Option<&'a Symbol> {
    let mut best: Option<&Symbol> = None;
    for s in &a.symbols {
        if s.name != name {
            continue;
        }
        match s.scope {
            Some((from, to)) if line >= from && line <= to => {
                // The declaration closest above the use wins.
                let better = match best {
                    Some(b) => b.scope.is_none() || s.line > b.line,
                    None => true,
                };
                if better {
                    best = Some(s);
                }
            }
            None => {
                if best.is_none() {
                    best = Some(s);
                }
            }
            _ => {}
        }
    }
    best
}

// ------------------------------------------------------------ the protocol

const KEYWORDS: &[&str] = &[
    "fn", "let", "var", "if", "else", "while", "return", "struct", "const", "profile", "as", "mut",
    "true", "false", "syscall", "extern", "break", "continue", "defer", "errdefer", "comptime",
    "for", "in", "import", "export", "enum", "match", "error", "try", "catch",
];

struct Server {
    docs: HashMap<String, String>,
    out: std::io::Stdout,
}

fn uri_to_path(uri: &str) -> String {
    let p = uri.strip_prefix("file://").unwrap_or(uri);
    // Percent decoding, the little that appears in paths.
    let b = p.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&p[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn position(line: u32, col: u32) -> Json {
    obj(vec![
        ("line", num(line.saturating_sub(1) as i64)),
        ("character", num(col.saturating_sub(1) as i64)),
    ])
}

fn range(line: u32, col: u32, len: u32) -> Json {
    obj(vec![
        ("start", position(line, col)),
        ("end", position(line, col + len)),
    ])
}

impl Server {
    fn send(&mut self, v: &Json) {
        let body = v.to_text();
        let head = format!("Content-Length: {}\r\n\r\n", body.len());
        let _ = self.out.write_all(head.as_bytes());
        let _ = self.out.write_all(body.as_bytes());
        let _ = self.out.flush();
    }

    fn answer(&mut self, id: &Json, result: Json) {
        self.send(&obj(vec![
            ("jsonrpc", text("2.0")),
            ("id", id.clone()),
            ("result", result),
        ]));
    }

    fn publish(&mut self, uri: &str) {
        let src = match self.docs.get(uri) {
            Some(s) => s.clone(),
            None => return,
        };
        let a = analyse(&uri_to_path(uri), &src);
        let list: Vec<Json> = a
            .diagnostics
            .iter()
            .map(|(sp, msg, help)| {
                let full = match help {
                    Some(h) => format!("{}\nhelp: {}", msg, h),
                    None => msg.clone(),
                };
                obj(vec![
                    ("range", range(sp.line.max(1), sp.col.max(1), sp.len.max(1))),
                    ("severity", num(1)),
                    ("source", text("firnc")),
                    ("message", text(&full)),
                ])
            })
            .collect();
        self.send(&obj(vec![
            ("jsonrpc", text("2.0")),
            ("method", text("textDocument/publishDiagnostics")),
            (
                "params",
                obj(vec![("uri", text(uri)), ("diagnostics", Json::Arr(list))]),
            ),
        ]));
    }

    fn handle(&mut self, msg: &Json) -> bool {
        let method = msg.get("method").map(|m| m.text().to_string()).unwrap_or_default();
        let id = msg.get("id").cloned();
        match method.as_str() {
            "initialize" => {
                let caps = obj(vec![
                    // 1 = full text on every change. Firn files are small, and
                    // a wrong incremental update would be worse than a slow one.
                    ("textDocumentSync", num(1)),
                    ("definitionProvider", Json::Bool(true)),
                    ("renameProvider", Json::Bool(true)),
                    ("hoverProvider", Json::Bool(true)),
                    ("documentFormattingProvider", Json::Bool(true)),
                    (
                        "completionProvider",
                        obj(vec![("resolveProvider", Json::Bool(false))]),
                    ),
                ]);
                let r = obj(vec![
                    ("capabilities", caps),
                    (
                        "serverInfo",
                        obj(vec![
                            ("name", text(&crate::config::compiler_name())),
                            ("version", text(crate::config::VERSION)),
                        ]),
                    ),
                ]);
                if let Some(i) = id {
                    self.answer(&i, r);
                }
            }
            "shutdown" => {
                if let Some(i) = id {
                    self.answer(&i, Json::Null);
                }
            }
            "exit" => return false,
            "textDocument/didOpen" => {
                let uri = msg.path(&["params", "textDocument", "uri"]).map(|u| u.text().to_string());
                let txt = msg.path(&["params", "textDocument", "text"]).map(|u| u.text().to_string());
                if let (Some(u), Some(t)) = (uri, txt) {
                    self.docs.insert(u.clone(), t);
                    self.publish(&u);
                }
            }
            "textDocument/didChange" => {
                let uri = msg.path(&["params", "textDocument", "uri"]).map(|u| u.text().to_string());
                let txt = match msg.path(&["params", "contentChanges"]) {
                    Some(Json::Arr(a)) => a
                        .last()
                        .and_then(|c| c.get("text"))
                        .map(|t| t.text().to_string()),
                    _ => None,
                };
                if let (Some(u), Some(t)) = (uri, txt) {
                    self.docs.insert(u.clone(), t);
                    self.publish(&u);
                }
            }
            "textDocument/didClose" => {
                if let Some(u) = msg.path(&["params", "textDocument", "uri"]) {
                    self.docs.remove(u.text());
                }
            }
            "textDocument/definition" => {
                let r = self.definition(msg);
                if let Some(i) = id {
                    self.answer(&i, r);
                }
            }
            "textDocument/hover" => {
                let r = self.hover(msg);
                if let Some(i) = id {
                    self.answer(&i, r);
                }
            }
            "textDocument/completion" => {
                let r = self.completion(msg);
                if let Some(i) = id {
                    self.answer(&i, r);
                }
            }
            "textDocument/rename" => {
                let r = self.rename(msg);
                if let Some(i) = id {
                    self.answer(&i, r);
                }
            }
            "textDocument/formatting" => {
                let r = self.formatting(msg);
                if let Some(i) = id {
                    self.answer(&i, r);
                }
            }
            _ => {
                // Unknown REQUEST (with an id) must be answered, otherwise
                // the editor waits forever. A notification must not be.
                if let Some(i) = id {
                    self.answer(&i, Json::Null);
                }
            }
        }
        true
    }

    /// (uri, source text, line, character) out of a `TextDocumentPositionParams`.
    fn place(&self, msg: &Json) -> Option<(String, String, u32, u32)> {
        let uri = msg.path(&["params", "textDocument", "uri"])?.text().to_string();
        let src = self.docs.get(&uri)?.clone();
        let line = msg.path(&["params", "position", "line"])?.number() as u32;
        let ch = msg.path(&["params", "position", "character"])?.number() as u32;
        Some((uri, src, line, ch))
    }

    fn definition(&mut self, msg: &Json) -> Json {
        let (uri, src, line, ch) = match self.place(msg) {
            Some(p) => p,
            None => return Json::Null,
        };
        let (name, at_line, _, _) = match name_at(&src, line, ch) {
            Some(n) => n,
            None => return Json::Null,
        };
        let a = analyse(&uri_to_path(&uri), &src);
        match resolve(&a, &name, at_line) {
            Some(s) => obj(vec![
                ("uri", text(&uri)),
                ("range", range(s.line, s.col, s.name.chars().count() as u32)),
            ]),
            None => Json::Null,
        }
    }

    fn hover(&mut self, msg: &Json) -> Json {
        let (uri, src, line, ch) = match self.place(msg) {
            Some(p) => p,
            None => return Json::Null,
        };
        let (name, at_line, col, len) = match name_at(&src, line, ch) {
            Some(n) => n,
            None => return Json::Null,
        };
        let a = analyse(&uri_to_path(&uri), &src);
        match resolve(&a, &name, at_line) {
            Some(s) => obj(vec![
                (
                    "contents",
                    obj(vec![
                        ("kind", text("plaintext")),
                        ("value", text(&format!("{}: {}", s.kind, s.about))),
                    ]),
                ),
                ("range", range(at_line, col, len)),
            ]),
            None => Json::Null,
        }
    }

    fn completion(&mut self, msg: &Json) -> Json {
        let (uri, src, line, ch) = match self.place(msg) {
            Some(p) => p,
            None => return Json::Null,
        };
        let a = analyse(&uri_to_path(&uri), &src);
        let at = line + 1;
        let mut items: Vec<Json> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for s in &a.symbols {
            if let Some((from, to)) = s.scope {
                if at < from || at > to {
                    continue;
                }
            }
            if seen.contains(&s.name) {
                continue;
            }
            seen.push(s.name.clone());
            // LSP CompletionItemKind: 3 = function, 22 = struct, 21 = constant,
            // 6 = variable, 9 = module
            let kind = match s.kind {
                "function" => 3,
                "struct" => 22,
                "const" => 21,
                "module" => 9,
                _ => 6,
            };
            items.push(obj(vec![
                ("label", text(&s.name)),
                ("kind", num(kind)),
                ("detail", text(s.about.as_str())),
            ]));
        }
        for k in KEYWORDS {
            items.push(obj(vec![
                ("label", text(k)),
                ("kind", num(14)), // keyword
                ("detail", text("keyword")),
            ]));
        }
        let _ = ch;
        obj(vec![
            ("isIncomplete", Json::Bool(false)),
            ("items", Json::Arr(items)),
        ])
    }

    fn rename(&mut self, msg: &Json) -> Json {
        let (uri, src, line, ch) = match self.place(msg) {
            Some(p) => p,
            None => return Json::Null,
        };
        let fresh = match msg.path(&["params", "newName"]) {
            Some(n) => n.text().to_string(),
            None => return Json::Null,
        };
        let (name, at_line, _, _) = match name_at(&src, line, ch) {
            Some(n) => n,
            None => return Json::Null,
        };
        let a = analyse(&uri_to_path(&uri), &src);
        let scope = resolve(&a, &name, at_line).and_then(|s| s.scope);
        let edits: Vec<Json> = occurrences(&src, &name, scope)
            .into_iter()
            .map(|(l, c, len)| {
                obj(vec![
                    ("range", range(l, c, len)),
                    ("newText", text(&fresh)),
                ])
            })
            .collect();
        if edits.is_empty() {
            return Json::Null;
        }
        obj(vec![(
            "changes",
            Json::Obj(vec![(uri.clone(), Json::Arr(edits))]),
        )])
    }

    fn formatting(&mut self, msg: &Json) -> Json {
        let uri = match msg.path(&["params", "textDocument", "uri"]) {
            Some(u) => u.text().to_string(),
            None => return Json::Null,
        };
        let src = match self.docs.get(&uri) {
            Some(s) => s.clone(),
            None => return Json::Null,
        };
        // The shape comes from `firnfmt` -- the very same program the tree
        // is formatted with. The server does not implement a second set of
        // rules; it calls the tool. If the tool is not there, nothing is
        // formatted rather than something being formatted differently.
        let out = match run_formatter(&src) {
            Some(o) => o,
            None => return Json::Arr(Vec::new()),
        };
        if out == src {
            return Json::Arr(Vec::new());
        }
        let lines = src.lines().count() as u32 + 2;
        Json::Arr(vec![obj(vec![
            (
                "range",
                obj(vec![
                    ("start", position(1, 1)),
                    ("end", position(lines, 1)),
                ]),
            ),
            ("newText", text(&out)),
        ])])
    }
}

/// Calls `firnfmt` (built next to the compiler binary or in the project).
fn run_formatter(src: &str) -> Option<String> {
    let candidates = [
        std::env::var("FIRNFMT").unwrap_or_default(),
        "./.firnfmt".to_string(),
        "tools/fmt/.firnfmt".to_string(),
    ];
    for c in candidates.iter().filter(|c| !c.is_empty()) {
        if !std::path::Path::new(c).exists() {
            continue;
        }
        let mut child = std::process::Command::new(c)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .ok()?;
        child.stdin.as_mut()?.write_all(src.as_bytes()).ok()?;
        let out = child.wait_with_output().ok()?;
        if !out.status.success() {
            return None;
        }
        return String::from_utf8(out.stdout).ok();
    }
    None
}

/// The main loop: read a message, answer it. Ends with `exit`.
pub fn serve() -> i32 {
    let mut server = Server { docs: HashMap::new(), out: std::io::stdout() };
    let mut input = std::io::stdin();
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        // A message: `Content-Length: N\r\n\r\n` plus N octets.
        let head_end = loop {
            if let Some(p) = find(&buffer, b"\r\n\r\n") {
                break p;
            }
            let n = match input.read(&mut chunk) {
                Ok(0) => return 0,
                Ok(n) => n,
                Err(_) => return 1,
            };
            buffer.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
        let mut len = 0usize;
        for line in head.split("\r\n") {
            if let Some(v) = line.strip_prefix("Content-Length:") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
        let body_at = head_end + 4;
        while buffer.len() < body_at + len {
            let n = match input.read(&mut chunk) {
                Ok(0) => return 0,
                Ok(n) => n,
                Err(_) => return 1,
            };
            buffer.extend_from_slice(&chunk[..n]);
        }
        let body = String::from_utf8_lossy(&buffer[body_at..body_at + len]).to_string();
        buffer.drain(..body_at + len);
        if let Some(msg) = parse_json(&body) {
            if !server.handle(&msg) {
                return 0;
            }
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_reads_and_writes_itself() {
        let src = r#"{"a":1,"b":[true,null,"x\ny"],"c":{"d":-2.5}}"#;
        let v = parse_json(src).expect("parsed");
        assert_eq!(v.get("a").unwrap().number(), 1);
        assert_eq!(v.path(&["c", "d"]).unwrap(), &Json::Num(-2.5));
        match v.get("b").unwrap() {
            Json::Arr(a) => assert_eq!(a[2].text(), "x\ny"),
            _ => panic!("no array"),
        }
        // written out and read back in again gives the same tree
        let again = parse_json(&v.to_text()).expect("re-read");
        assert_eq!(v, again);
    }

    #[test]
    fn symbols_and_scopes() {
        let src = "fn add(a: i32, b: i32) -> i32 {\n    let s: i32 = a + b\n    return s\n}\n\nfn main() -> i32 {\n    let s: i32 = 1\n    return add(s, 2)\n}\n";
        let a = analyse("t.fi", src);
        assert!(a.diagnostics.is_empty(), "{:?}", a.diagnostics);
        // `s` exists twice; from line 3 the one out of `add` has to win.
        let s1 = resolve(&a, "s", 3).expect("s in add");
        assert_eq!(s1.line, 2);
        let s2 = resolve(&a, "s", 8).expect("s in main");
        assert_eq!(s2.line, 7);
        // `add` is global and reachable from everywhere
        assert_eq!(resolve(&a, "add", 8).unwrap().kind, "function");
    }

    #[test]
    fn rename_stays_in_the_scope() {
        let src = "fn f(x: i32) -> i32 {\n    return x\n}\n\nfn g(x: i32) -> i32 {\n    return x + 1\n}\n";
        let a = analyse("t.fi", src);
        let s = resolve(&a, "x", 2).expect("x in f");
        let places = occurrences(src, "x", s.scope);
        // only the two places in `f`, not the ones in `g`
        assert_eq!(places.len(), 2, "{:?}", places);
        assert!(places.iter().all(|(l, _, _)| *l <= 3));
    }

    #[test]
    fn a_field_is_no_name() {
        let src = "struct P { x: i32 }\n\nfn f(p: P) -> i32 {\n    let x: i32 = 1\n    return p.x + x\n}\n";
        let a = analyse("t.fi", src);
        let s = resolve(&a, "x", 5).expect("x");
        let places = occurrences(src, "x", s.scope);
        // `p.x` must NOT be renamed along -- that is a field.
        assert_eq!(places.len(), 2, "{:?}", places);
    }

    #[test]
    fn the_cursor_finds_the_name() {
        let src = "fn main() -> i32 {\n    let value: i32 = 1\n    return value\n}\n";
        // line 2 (0-based 1), on `value`
        let (n, l, c, len) = name_at(src, 1, 10).expect("name");
        assert_eq!((n.as_str(), l, c, len), ("value", 2, 9, 5));
        // in the blank in front of it there is nothing
        assert!(name_at(src, 1, 3).is_none());
    }

    #[test]
    fn diagnostics_carry_the_suggestion() {
        let src = "fn main() -> i32 {\n    let value: i32 = 1\n    return valu\n}\n";
        let a = analyse("t.fi", src);
        assert_eq!(a.diagnostics.len(), 1);
        let (sp, msg, help) = &a.diagnostics[0];
        assert_eq!(sp.line, 3);
        assert!(msg.contains("unknown name 'valu'"), "{}", msg);
        assert_eq!(help.as_deref(), Some("did you mean 'value'?"));
    }
}
