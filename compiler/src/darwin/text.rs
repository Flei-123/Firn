// SPDX-License-Identifier: MPL-2.0
use std::collections::HashSet;

const LOCAL_PREFIX: &str = ".L";
const DARWIN_LOCAL_PREFIX: &str = "L";
const SYSCALL_INSTRUCTION: &str = "svc #0";
const SYSCALL_SEQUENCE: &str = "    str x30, [sp, #-16]!\n    bl __Fmac.syscall\n    ldr x30, [sp], #16";

const SYMBOLIC_MNEMONICS: &[&str] = &["bl", "b", "adrp", "adr"];

const RESERVED: &[&str] = &[
    "eq", "ne", "cs", "hs", "cc", "lo", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "al", "nv", "lsl",
    "lsr", "asr", "ror", "msl", "uxtb", "uxth", "uxtw", "uxtx", "sxtb", "sxth", "sxtw", "sxtx", "sp", "wsp", "xzr",
    "wzr", "fp", "lr",
];

const SECTION_MAP: &[(&str, &str)] = &[
    (".rodata", ".section __TEXT,__const"),
    (".text", ".text"),
    (".data", ".data"),
    (".data.rel.ro", ".section __DATA,__const"),
    (".bss", ".section __DATA,__bss"),
];

pub fn translate(asm: &str) -> String {
    let symbols = collect_symbols(asm);
    let mut out = String::with_capacity(asm.len() + asm.len() / 8);
    for line in asm.lines() {
        if let Some(text) = translate_line(line, &symbols) {
            out.push_str(&text);
            out.push('\n');
        }
    }
    out
}

fn collect_symbols(asm: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in asm.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(".globl ") {
            set.insert(rest.trim().to_string());
        } else if let Some(name) = t.strip_suffix(':') {
            if is_global_label(name) {
                set.insert(name.to_string());
            }
        }
    }
    set
}

fn is_global_label(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(LOCAL_PREFIX)
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && !name.contains(char::is_whitespace)
}

fn translate_line(line: &str, symbols: &HashSet<String>) -> Option<String> {
    let t = line.trim();
    if t.is_empty() {
        return Some(String::new());
    }
    if t == SYSCALL_INSTRUCTION {
        return Some(SYSCALL_SEQUENCE.to_string());
    }
    if t == "_start:" {
        return Some(super::entry::ENTRY_PROLOGUE.trim_end().to_string());
    }
    if t.starts_with(".arch ") || t.starts_with(".section .note") {
        return None;
    }
    if t.starts_with(".section ") || t == ".text" || t == ".data" {
        return Some(map_section(t));
    }
    let body = strip_comment(line);
    let (indent, rest) = split_indent(&body);
    if rest.is_empty() {
        return Some(String::new());
    }
    let is_instruction = !rest.starts_with('.') && !rest.ends_with(':');
    let prepared = rewrite_relocations(rest);
    let renamed = rename_tokens(&prepared, symbols, is_instruction);
    let renamed = if is_instruction && mnemonic(&renamed) == "adrp" {
        format!("{}@PAGE", renamed.trim_end())
    } else {
        renamed
    };
    Some(format!("{}{}", indent, renamed))
}

fn map_section(t: &str) -> String {
    let name = t
        .trim_start_matches(".section")
        .trim()
        .split(',')
        .next()
        .unwrap_or("")
        .trim();
    let name = if t == ".text" { ".text" } else if t == ".data" { ".data" } else { name };
    for (from, to) in SECTION_MAP {
        if name == *from {
            return to.to_string();
        }
    }
    ".text".to_string()
}

fn strip_comment(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'/' if !in_str && bytes.get(i + 1) == Some(&b'/') => return line[..i].to_string(),
            _ => {}
        }
        i += 1;
    }
    line.to_string()
}

fn split_indent(line: &str) -> (&str, &str) {
    let rest = line.trim_start();
    let indent = &line[..line.len() - rest.len()];
    (indent, rest.trim_end())
}

fn is_reserved(token: &str) -> bool {
    if RESERVED.contains(&token) {
        return true;
    }
    let mut chars = token.chars();
    let head = chars.next().unwrap_or(' ');
    let tail = chars.as_str();
    matches!(head, 'x' | 'w' | 'v' | 'q' | 'd' | 's' | 'b' | 'h') && !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit())
}

fn mnemonic(text: &str) -> &str {
    text.split_whitespace().next().unwrap_or("")
}

fn rewrite_relocations(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(pos) = rest.find(":lo12:") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + ":lo12:".len()..];
        let end = after.find(|c: char| !is_ident_char(c)).unwrap_or(after.len());
        out.push_str(&after[..end]);
        out.push_str("@PAGEOFF");
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '.'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$'
}

fn rename_tokens(text: &str, symbols: &HashSet<String>, skip_mnemonic: bool) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut in_str = false;
    let mut first = skip_mnemonic;
    let symbolic = !skip_mnemonic || SYMBOLIC_MNEMONICS.contains(&mnemonic(text));
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 1;
            } else if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        let after_hash = i > 0 && chars[i - 1] == '#';
        if is_ident_start(c) && !after_hash {
            let start = i;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            if first {
                first = false;
                out.push_str(&token);
            } else if let Some(rest) = token.strip_prefix(LOCAL_PREFIX) {
                out.push_str(DARWIN_LOCAL_PREFIX);
                out.push_str(rest);
            } else if symbols.contains(&token) && (symbolic || !is_reserved(&token)) {
                out.push('_');
                out.push_str(&token);
            } else {
                out.push_str(&token);
            }
            continue;
        }
        if c.is_ascii_digit() {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::translate;

    fn one(asm: &str) -> String {
        translate(asm)
    }

    #[test]
    fn globals_get_an_underscore_and_locals_lose_the_dot() {
        let out = one(".globl foo\nfoo:\n.Lfoo__bb0:\n    bl foo\n    b .Lfoo__bb0\n");
        assert!(out.contains(".globl _foo"), "{}", out);
        assert!(out.contains("_foo:"), "{}", out);
        assert!(out.contains("Lfoo__bb0:"), "{}", out);
        assert!(out.contains("bl _foo"), "{}", out);
        assert!(out.contains("b Lfoo__bb0"), "{}", out);
    }

    #[test]
    fn page_relocations_use_the_apple_spelling() {
        let out = one(".Lmsg:\n    adrp x9, .Lmsg\n    add x9, x9, :lo12:.Lmsg\n");
        assert!(out.contains("adrp x9, Lmsg@PAGE"), "{}", out);
        assert!(out.contains("add x9, x9, Lmsg@PAGEOFF"), "{}", out);
    }

    #[test]
    fn syscalls_go_through_the_stub() {
        let out = one("    svc #0\n");
        assert!(out.contains("bl __Fmac.syscall"), "{}", out);
        assert!(!out.contains("svc"), "{}", out);
    }

    #[test]
    fn a_function_named_like_a_condition_is_not_confused() {
        let out = one(".globl eq\neq:\n    csel x0, x1, x2, eq\n    bl eq\n");
        assert!(out.contains("csel x0, x1, x2, eq"), "{}", out);
        assert!(out.contains("bl _eq"), "{}", out);
    }

    #[test]
    fn string_contents_are_untouched() {
        let out = one(".globl foo\nfoo:\n.ascii \"foo .Lbar\"\n");
        assert!(out.contains(".ascii \"foo .Lbar\""), "{}", out);
    }

    #[test]
    fn sections_are_mapped_and_gnu_notes_dropped() {
        let out = one(".section .rodata\n.section .bss,\"aw\",@nobits\n.section .note.GNU-stack,\"\",%progbits\n");
        assert!(out.contains(".section __TEXT,__const"), "{}", out);
        assert!(out.contains(".section __DATA,__bss"), "{}", out);
        assert!(!out.contains("note"), "{}", out);
    }

    #[test]
    fn the_start_label_becomes_the_argument_shim() {
        let out = one(".globl _start\n_start:\n    bl main\n");
        assert!(out.contains("__start:"), "{}", out);
        assert!(out.contains("Lfmac_args_copy"), "{}", out);
    }
}
