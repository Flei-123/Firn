pub(crate) const GPR: [[&str; 4]; 16] = [
    ["rax", "eax", "ax", "al"],
    ["rcx", "ecx", "cx", "cl"],
    ["rdx", "edx", "dx", "dl"],
    ["rbx", "ebx", "bx", "bl"],
    ["rsp", "esp", "sp", "spl"],
    ["rbp", "ebp", "bp", "bpl"],
    ["rsi", "esi", "si", "sil"],
    ["rdi", "edi", "di", "dil"],
    ["r8", "r8d", "r8w", "r8b"],
    ["r9", "r9d", "r9w", "r9b"],
    ["r10", "r10d", "r10w", "r10b"],
    ["r11", "r11d", "r11w", "r11b"],
    ["r12", "r12d", "r12w", "r12b"],
    ["r13", "r13d", "r13w", "r13b"],
    ["r14", "r14d", "r14w", "r14b"],
    ["r15", "r15d", "r15w", "r15b"],
];

const SUFFIX: [&str; 4] = ["", "d", "w", "b"];

const WINDOW_OPS: [&str; 20] = [
    "add", "sub", "and", "or", "xor", "shl", "shr", "sar", "rol", "ror", "neg", "not", "inc", "dec", "mov",
    "movzx", "movsx", "movsxd", "lea", "imul",
];

pub(crate) fn parse_reg(tok: &str) -> Option<(usize, usize)> {
    for (i, names) in GPR.iter().enumerate() {
        for (w, n) in names.iter().enumerate() {
            if *n == tok {
                return Some((i, w));
            }
        }
    }
    None
}

pub(crate) fn split_line(line: &str) -> Option<(&str, Vec<&str>)> {
    let mut s = line.trim();
    if s.is_empty() || s.starts_with('#') {
        return None;
    }
    if let Some(p) = s.find(" #") {
        s = s[..p].trim();
    }
    let (mn, rest) = match s.find(char::is_whitespace) {
        Some(p) => (&s[..p], s[p..].trim()),
        None => (s, ""),
    };
    let ops = if rest.is_empty() { Vec::new() } else { rest.split(',').map(|o| o.trim()).collect() };
    Some((mn, ops))
}

pub(crate) fn parse_number(s: &str) -> Option<i128> {
    let (neg, body) = match s.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, s),
    };
    let v = if let Some(h) = body.strip_prefix("0x") {
        if h.is_empty() || h.len() > 16 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        i128::from_str_radix(h, 16).ok()?
    } else {
        if body.is_empty() || body.len() > 20 || !body.chars().all(|c| c.is_ascii_digit()) || (body.len() > 1 && body.starts_with('0')) {
            return None;
        }
        body.parse::<i128>().ok()?
    };
    Some(if neg { -v } else { v })
}

fn ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '.' || c == '$'
}

fn ident_char(c: char) -> bool {
    ident_start(c) || c.is_ascii_digit()
}

fn rename(op: &str, regs: &mut Vec<usize>, out: &mut String) -> bool {
    let chars: Vec<char> = op.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !ident_start(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && ident_char(chars[i]) {
            i += 1;
        }
        let tok: String = chars[start..i].iter().collect();
        let Some((idx, w)) = parse_reg(&tok) else { return false };
        if idx == 4 || idx == 5 {
            return false;
        }
        let var = match regs.iter().position(|&r| r == idx) {
            Some(v) => v,
            None => {
                regs.push(idx);
                regs.len() - 1
            }
        };
        out.push('%');
        out.push_str(&var.to_string());
        out.push_str(SUFFIX[w]);
    }
    true
}

fn normalize_line(line: &str, regs: &mut Vec<usize>, out: &mut String) -> bool {
    let Some((mn, ops)) = split_line(line) else { return false };
    if !WINDOW_OPS.contains(&mn) || line.contains("ptr") {
        return false;
    }
    if mn != "lea" && ops.iter().any(|o| o.contains('[')) {
        return false;
    }
    if mn == "lea" && (ops.len() != 2 || !ops[1].starts_with('[')) {
        return false;
    }
    if mn == "imul" && ops.len() != 2 && ops.len() != 3 {
        return false;
    }
    out.push_str(mn);
    for (n, o) in ops.iter().enumerate() {
        out.push_str(if n == 0 { " " } else { ", " });
        if !o.starts_with('[') {
            if let Some(v) = parse_number(o) {
                out.push_str(&v.to_string());
                continue;
            }
        }
        if !rename(o, regs, out) {
            return false;
        }
    }
    true
}

pub(crate) fn eligible(line: &str) -> bool {
    let mut regs = Vec::new();
    let mut s = String::new();
    normalize_line(line, &mut regs, &mut s)
}

pub(crate) fn normalize(lines: &[&str]) -> Option<(String, Vec<usize>)> {
    let mut regs = Vec::new();
    let mut key = String::new();
    for (n, l) in lines.iter().enumerate() {
        if n > 0 {
            key.push('\n');
        }
        if !normalize_line(l, &mut regs, &mut key) {
            return None;
        }
    }
    Some((key, regs))
}

pub(crate) fn prefixes(lines: &[&str], max: usize) -> Vec<(usize, String, Vec<usize>)> {
    let mut regs = Vec::new();
    let mut key = String::new();
    let mut out = Vec::new();
    for (n, l) in lines.iter().take(max).enumerate() {
        if n > 0 {
            key.push('\n');
        }
        if !normalize_line(l, &mut regs, &mut key) {
            break;
        }
        out.push((n + 1, key.clone(), regs.clone()));
    }
    out
}

pub(crate) fn render(template: &str, regs: &[usize]) -> Option<String> {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let var: usize = chars[start..i].iter().collect::<String>().parse().ok()?;
        let w = match chars.get(i) {
            Some('d') => 1,
            Some('w') => 2,
            Some('b') => 3,
            _ => 0,
        };
        if w > 0 {
            i += 1;
        }
        out.push_str(GPR[*regs.get(var)?][w]);
    }
    Some(out)
}
