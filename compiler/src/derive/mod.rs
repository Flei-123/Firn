// SPDX-License-Identifier: MPL-2.0
mod decl;
mod emit;
mod emit_enum;
mod expr;

pub struct Fail {
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub msg: String,
}

const ATTR_OPEN: &str = "#[derive(";
const IMPORT_TEXT: &str = "import std.derive; ";
const KNOWN: &[&str] = &["Debug", "Eq", "Ord", "Hash", "Clone", "Default", "Json"];

fn position(src: &str, at: usize) -> (u32, u32) {
    let before = &src[..at];
    let line = before.matches('\n').count() as u32 + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) as u32 + 1;
    (line, col)
}

fn fail(src: &str, at: usize, len: usize, msg: String) -> Fail {
    let (line, col) = position(src, at);
    Fail { line, col, len: len as u32, msg }
}

fn parse_names(src: &str, attr: usize) -> Result<(Vec<String>, usize), Fail> {
    let start = attr + ATTR_OPEN.len();
    let Some(rel) = src[start..].find(")]") else {
        return Err(fail(src, attr, ATTR_OPEN.len(), "unterminated derive attribute, expected ')]'".to_string()));
    };
    let names: Vec<String> = src[start..start + rel].split(',').map(|s| s.trim().to_string()).collect();
    for n in &names {
        if !KNOWN.contains(&n.as_str()) {
            let msg = format!("unknown derive '{}', known: {}", n, KNOWN.join(", "));
            return Err(fail(src, attr, ATTR_OPEN.len() + rel + 2, msg));
        }
    }
    Ok((names, start + rel + 2))
}

pub fn expand(src: &str) -> Result<Option<String>, Fail> {
    if !src.contains(ATTR_OPEN) {
        return Ok(None);
    }
    let mut blanks: Vec<(usize, usize)> = Vec::new();
    let mut inserts: Vec<(usize, String)> = Vec::new();
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        let lead = line.len() - line.trim_start().len();
        if line[lead..].starts_with(ATTR_OPEN) {
            let attr = offset + lead;
            let (names, attr_end) = parse_names(src, attr)?;
            let decl = decl::parse(src, attr, attr_end)?;
            let text = emit::generate(&decl, &names).map_err(|m| fail(src, attr, ATTR_OPEN.len(), m))?;
            blanks.push((attr, attr_end));
            inserts.push((decl.end, text));
        }
        offset += line.len();
    }
    Ok(Some(apply(src, blanks, inserts)))
}

fn apply(src: &str, blanks: Vec<(usize, usize)>, inserts: Vec<(usize, String)>) -> String {
    let mut events: Vec<(usize, usize, String)> = Vec::new();
    for (s, e) in blanks {
        events.push((s, e, " ".repeat(e - s)));
    }
    for (at, text) in inserts {
        events.push((at, at, text));
    }
    events.sort_by_key(|e| (e.0, e.1));
    let mut out = String::with_capacity(src.len() + 1024);
    let mut cursor = 0;
    let mut import_done = src.contains("import std.derive");
    for (s, e, text) in events {
        out.push_str(&src[cursor..s]);
        if s == e && !import_done {
            out.push_str(IMPORT_TEXT);
            import_done = true;
        }
        out.push_str(&text);
        cursor = e;
    }
    out.push_str(&src[cursor..]);
    out
}
