// SPDX-License-Identifier: MPL-2.0
use std::collections::HashSet;

const SCRATCH: &[&str] = &["r11", "r10", "r9", "r8"];

pub fn rewrite(rest: &str, symbols: &HashSet<String>) -> Option<Vec<String>> {
    let open = rest.find('[')?;
    let close = rest[open..].find(']')? + open;
    let inner = rest[open + 1..close].trim_start();
    let end = inner.find(|c: char| !is_ident_char(c)).unwrap_or(inner.len());
    let symbol = &inner[..end];
    if symbol.is_empty() || !(symbol.starts_with(".L") || symbols.contains(symbol)) {
        return None;
    }
    if matches!(rest.split_whitespace().next(), Some("jmp" | "call")) {
        return None;
    }
    let temp = SCRATCH.iter().find(|r| !rest.contains(*r))?;
    let tail = &inner[end..];
    let access = format!("{}[{}{}{}", &rest[..open], temp, tail, &rest[close..]);
    Some(vec![
        format!("push {}", temp),
        format!("lea {}, [rip+{}]", temp, symbol),
        access,
        format!("pop {}", temp),
    ])
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$'
}
