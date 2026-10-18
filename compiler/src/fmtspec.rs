// SPDX-License-Identifier: MPL-2.0
//! r665 -- format specs of the f-string: `f"{x:>8.2f}"`.
//!
//! The parser splits `expr:spec` at the first `:` that is outside of
//! brackets, quotes and `::`, and packs the spec into ONE u64 literal that
//! travels as the third argument of the placeholder `io.fmt_spec(chain, x, spec)`.
//! The type check / lowering pick the runtime step by the type of `x`
//! (`io.fmt_spec_i / _u / _f64 / _f32 / _str / _bool`).
//!
//! Packing (the library `lib/std/io.fi` decodes the same layout):
//!   bits  0..7   fill octet (default 32)
//!   bits  8..10  align: 0 none, 1 `<`, 2 `>`, 3 `^`, 4 `=`
//!   bit   11     sign `+`;  bit 12 sign ` `;  bit 13 alt `#`
//!   bits 16..31  width
//!   bits 32..47  precision + 1 (0 = none)
//!   bits 48..55  type octet (0 none, d x X o b f F s %)
//!
//! Python's grammar `[[fill]align][sign][#][0][width][.precision][type]`
//! without `,` `_` `e` `g` `c` and without non-ASCII fill.

/// Index of the spec colon in the expression text, if any.
pub(crate) fn split_colon(raw: &[char]) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut i: usize = 0;
    while i < raw.len() {
        let c = raw[i];
        if c == '"' {
            i += 1;
            while i < raw.len() && raw[i] != '"' {
                if raw[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if c == '(' || c == '[' {
            depth += 1;
        } else if c == ')' || c == ']' {
            depth -= 1;
        } else if c == ':' && depth == 0 {
            if raw.get(i + 1) == Some(&':') {
                i += 2;
                continue;
            }
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Parse the text after the colon into the packed u64.
pub(crate) fn parse_spec(s: &[char]) -> Result<u64, String> {
    let mut i: usize = 0;
    let mut fill: u64 = 32;
    let mut fill_set = false;
    let mut align: u64 = 0;
    let is_align = |c: char| matches!(c, '<' | '>' | '^' | '=');
    let code = |c: char| -> u64 {
        match c {
            '<' => 1,
            '>' => 2,
            '^' => 3,
            _ => 4,
        }
    };
    if s.len() >= 2 && is_align(s[1]) {
        if !s[0].is_ascii() {
            return Err("the fill character must be ASCII".to_string());
        }
        fill = s[0] as u64;
        fill_set = true;
        align = code(s[1]);
        i = 2;
    } else if !s.is_empty() && is_align(s[0]) {
        align = code(s[0]);
        i = 1;
    }
    let mut flags: u64 = 0;
    if i < s.len() && (s[i] == '+' || s[i] == ' ') {
        flags |= if s[i] == '+' { 1 << 11 } else { 1 << 12 };
        i += 1;
    } else if i < s.len() && s[i] == '-' {
        i += 1;
    }
    if i < s.len() && s[i] == '#' {
        flags |= 1 << 13;
        i += 1;
    }
    if i < s.len() && s[i] == '0' {
        if !fill_set {
            fill = 48;
        }
        if align == 0 {
            align = 4;
        }
        i += 1;
    }
    let mut width: u64 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        width = width * 10 + s[i] as u64 - 48;
        if width > 65535 {
            return Err("the width is too large (max 65535)".to_string());
        }
        i += 1;
    }
    let mut prec1: u64 = 0;
    if i < s.len() && s[i] == '.' {
        i += 1;
        let start = i;
        let mut p: u64 = 0;
        while i < s.len() && s[i].is_ascii_digit() {
            p = p * 10 + s[i] as u64 - 48;
            if p > 65534 {
                return Err("the precision is too large (max 65534)".to_string());
            }
            i += 1;
        }
        if i == start {
            return Err("a '.' needs a precision (digits)".to_string());
        }
        prec1 = p + 1;
    }
    let mut ty: u64 = 0;
    if i < s.len() {
        let c = s[i];
        if !matches!(c, 'd' | 'x' | 'X' | 'o' | 'b' | 'f' | 'F' | 's' | '%') {
            return Err(format!(
                "unsupported format spec text '{}' (supported: [[fill]align][sign][#][0][width][.precision][d x X o b f F s %])",
                s[i..].iter().collect::<String>()
            ));
        }
        ty = c as u64;
        i += 1;
    }
    if i < s.len() {
        return Err(format!(
            "unexpected '{}' at the end of a format spec",
            s[i..].iter().collect::<String>()
        ));
    }
    Ok(fill | (align << 8) | flags | (width << 16) | (prec1 << 32) | (ty << 48))
}

/// Type-specific validity of a spec, decided where the type is known.
/// `kind`: 'i' signed/unsigned integer, 'f' float, 's' str, 'b' bool.
pub(crate) fn check_for(kind: char, spec: u64) -> Result<(), String> {
    let ty = ((spec >> 48) & 255) as u8 as char;
    let prec = (spec >> 32) & 0xFFFF;
    let align = (spec >> 8) & 7;
    match kind {
        'i' => {
            if !matches!(ty, '\0' | 'd' | 'x' | 'X' | 'o' | 'b' | 'f' | 'F' | '%') {
                return Err(format!("format type '{}' does not fit an integer", ty));
            }
            if prec != 0 && !matches!(ty, 'f' | 'F' | '%') {
                return Err("an integer takes no precision (use :f / :% or none)".to_string());
            }
        }
        'f' => {
            if !matches!(ty, '\0' | 'f' | 'F' | '%') {
                return Err(format!("format type '{}' does not fit a float", ty));
            }
            if prec != 0 && ty == '\0' {
                return Err("a float precision needs the type f: `{x:.2f}`".to_string());
            }
        }
        _ => {
            if !matches!(ty, '\0' | 's') {
                return Err(format!("format type '{}' does not fit a str/bool", ty));
            }
            if align == 4 || (spec >> 11) & 7 != 0 {
                return Err("a str takes no sign and no `=` alignment".to_string());
            }
        }
    }
    Ok(())
}
