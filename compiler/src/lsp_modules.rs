// SPDX-License-Identifier: MPL-2.0
use std::path::{Path, PathBuf};

use crate::diag::Diags;
use crate::lexer;
use crate::lsp::Symbol;

pub struct Member {
    pub name: String,
    pub kind: &'static str,
    pub detail: String,
    pub line: u32,
    pub col: u32,
}

pub fn module_file(doc_path: &str, import_path: &[String]) -> Option<PathBuf> {
    let relative = format!("{}.fi", import_path.join("/"));
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(dir) = Path::new(doc_path).parent() {
        roots.push(dir.to_path_buf());
    }
    if let Ok(lib) = std::env::var("FIRNLIB") {
        roots.extend(lib.split(if cfg!(windows) { ';' } else { ':' }).map(PathBuf::from));
    }
    if let Some(lib) = crate::modules::sysroot_lib() {
        roots.push(lib);
    }
    roots.into_iter().map(|r| r.join(&relative)).find(|c| c.is_file())
}

pub fn members_of(file: &Path) -> Vec<Member> {
    let Ok(src) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let mut dg = Diags::new(&file.display().to_string(), &src);
    let toks = lexer::lex(&src, &mut dg);
    let prog = crate::parser::parse(&toks, &mut dg);
    let exported = |name: &str| prog.exports.is_empty() || prog.exports.iter().any(|(n, _)| n == name);
    let mut out: Vec<Member> = Vec::new();
    for f in prog.funcs.iter().filter(|f| exported(&f.name)) {
        let params: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
        out.push(Member {
            name: f.name.clone(),
            kind: "function",
            detail: format!("fn {}({})", f.name, params.join(", ")),
            line: f.span.line,
            col: f.span.col,
        });
    }
    for s in prog.structs.iter().filter(|s| exported(&s.name)) {
        out.push(Member {
            name: s.name.clone(),
            kind: "struct",
            detail: format!("struct {}", s.name),
            line: s.span.line,
            col: s.span.col,
        });
    }
    for c in prog.consts.iter().filter(|c| exported(&c.name)) {
        out.push(Member {
            name: c.name.clone(),
            kind: "const",
            detail: format!("const {}", c.name),
            line: c.span.line,
            col: c.span.col,
        });
    }
    out
}

pub fn imported_path(symbols: &[Symbol], alias: &str) -> Option<Vec<String>> {
    let symbol = symbols.iter().find(|s| s.kind == "module" && s.name == alias)?;
    let path = symbol.about.strip_prefix("import ")?;
    Some(path.split('.').map(|p| p.to_string()).collect())
}

pub fn members_for_alias(doc_path: &str, symbols: &[Symbol], alias: &str) -> Option<(PathBuf, Vec<Member>)> {
    let file = module_file(doc_path, &imported_path(symbols, alias)?)?;
    let members = members_of(&file);
    Some((file, members))
}

pub fn locate_member(
    doc_path: &str,
    src: &str,
    symbols: &[Symbol],
    line0: u32,
    char0: u32,
    name: &str,
) -> Option<(PathBuf, Member)> {
    let line_text = src.lines().nth(line0 as usize)?;
    let alias = qualifier_before(line_text, char0 as usize)?;
    let (file, members) = members_for_alias(doc_path, symbols, &alias)?;
    let member = members.into_iter().find(|m| m.name == name)?;
    Some((file, member))
}

pub fn qualifier_before(line_text: &str, char0: usize) -> Option<String> {
    let chars: Vec<char> = line_text.chars().collect();
    let mut i = char0.min(chars.len());
    while i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
        i -= 1;
    }
    if i == 0 || chars[i - 1] != '.' {
        return None;
    }
    let end = i - 1;
    let mut start = end;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }
    if start == end {
        return None;
    }
    Some(chars[start..end].iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualifier_is_found_before_the_dot() {
        assert_eq!(qualifier_before("    io.pri", 9), Some("io".to_string()));
        assert_eq!(qualifier_before("    io.", 7), Some("io".to_string()));
        assert_eq!(qualifier_before("    pri", 7), None);
    }
}
