use std::collections::HashMap;

const ALL: u16 = 0xFFFF;
const CALLER_SAVED_MASK: u16 = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 6) | (1 << 7) | (1 << 8) | (1 << 9) | (1 << 10) | (1 << 11);

const GPR_NAMES: [[&str; 4]; 16] = [
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

const WRITES_DEST: [&str; 36] = [
    "mov", "movzx", "lea", "add", "sub", "and", "or", "xor", "imul", "shl", "shr", "sar", "rol", "ror",
    "neg", "not", "inc", "dec", "adc", "sbb", "popcnt", "lzcnt", "tzcnt", "cmove", "cmovne", "cmovl",
    "cmovle", "cmovg", "cmovge", "cmovb", "cmovbe", "cmova", "cmovae", "movsx", "movsxd", "bswap",
];

const NO_EFFECT: [&str; 12] = [
    "cmp", "test", "push", "nop", "ucomisd", "ucomiss", "comisd", "comiss", "bt", "prefetcht0", "pause", "lfence",
];

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Known,
    Clear,
}

fn parse_reg(tok: &str) -> Option<(usize, usize)> {
    for (i, names) in GPR_NAMES.iter().enumerate() {
        for (w, n) in names.iter().enumerate() {
            if *n == tok {
                return Some((i, w));
            }
        }
    }
    match tok {
        "ah" => Some((0, 3)),
        "ch" => Some((1, 3)),
        "dh" => Some((2, 3)),
        "bh" => Some((3, 3)),
        _ => None,
    }
}

fn is_shift(m: &str) -> bool {
    matches!(m, "shl" | "shr" | "sar" | "rol" | "ror")
}

fn dest_class(m: &str, ops: &[&str]) -> Class {
    if is_shift(m) {
        let count = ops.get(1).copied().unwrap_or("");
        return match count.parse::<i64>() {
            Ok(n) if n != 0 => Class::Known,
            _ => Class::Clear,
        };
    }
    Class::Known
}

fn split_ops(rest: &str) -> Vec<&str> {
    rest.split(", ").map(|s| s.trim()).collect()
}

fn step(state: u16, line: &str) -> u16 {
    let t = line.trim();
    let (m, rest) = match t.split_once(' ') {
        Some((m, r)) => (m, r),
        None => (t, ""),
    };
    if m == "call" || m == "syscall" {
        return state & !CALLER_SAVED_MASK;
    }
    if NO_EFFECT.contains(&m) {
        return state;
    }
    if m == "pop" {
        return match parse_reg(rest.trim()) {
            Some((i, _)) => state & !(1 << i),
            None => state,
        };
    }
    let ops = split_ops(rest);
    let first = ops.first().copied().unwrap_or("");
    if m.starts_with("set") && m.len() <= 5 {
        return match parse_reg(first) {
            Some((i, _)) => state & !(1 << i),
            None => state,
        };
    }
    if WRITES_DEST.contains(&m) && !(m == "imul" && ops.len() == 1) {
        return match parse_reg(first) {
            Some((i, w)) => {
                if w == 1 && dest_class(m, &ops) == Class::Known {
                    state | (1 << i)
                } else {
                    state & !(1 << i)
                }
            }
            None => state,
        };
    }
    if matches!(m, "movd" | "cvttsd2si" | "cvttss2si" | "movmskpd" | "movmskps") {
        return match parse_reg(first) {
            Some((i, 1)) => state | (1 << i),
            Some((i, _)) => state & !(1 << i),
            None => state,
        };
    }
    if (first.starts_with("xmm") || first.starts_with("ymm")) && !m.starts_with("pcmp") && m != "maskmovdqu" {
        return state;
    }
    0
}

fn branch_target(m: &str, rest: &str) -> Option<Option<String>> {
    if !m.starts_with('j') {
        return None;
    }
    let target = rest.trim();
    if target.starts_with('.') || target.starts_with('_') {
        Some(Some(target.to_string()))
    } else {
        Some(None)
    }
}

enum Item<'a> {
    Label(&'a str),
    Instr(&'a str),
    Skip,
}

fn classify(line: &str) -> Option<Item<'_>> {
    let t = line.trim();
    if t.is_empty() || t.starts_with(".loc ") || t.starts_with(".globl ") || t.starts_with(".p2align") || t.starts_with(".balign") {
        return Some(Item::Skip);
    }
    if t.starts_with('.') && !t.ends_with(':') {
        return None;
    }
    if t.starts_with('#') {
        return Some(Item::Skip);
    }
    if t.ends_with(':') && !t.contains(' ') {
        return Some(Item::Label(&t[..t.len() - 1]));
    }
    if t.contains(".L") && !t.starts_with('j') {
        return None;
    }
    Some(Item::Instr(t))
}

fn same_reg32(line: &str) -> Option<usize> {
    let t = line.trim();
    let rest = t.strip_prefix("mov ")?;
    let (d, s) = rest.split_once(", ")?;
    if d != s {
        return None;
    }
    match parse_reg(d) {
        Some((i, 1)) => Some(i),
        _ => None,
    }
}

pub fn run(asm: &str) -> String {
    if std::env::var_os("FIRN_NO_ZEXTFLOW").is_some() {
        return asm.to_string();
    }
    let lines: Vec<&str> = asm.lines().collect();
    let mut items: Vec<Item> = Vec::with_capacity(lines.len());
    let mut any_candidate = false;
    for l in &lines {
        match classify(l) {
            Some(it) => {
                if same_reg32(l).is_some() {
                    any_candidate = true;
                }
                items.push(it);
            }
            None => return asm.to_string(),
        }
    }
    if !any_candidate {
        return asm.to_string();
    }
    let mut label_state: HashMap<&str, u16> = HashMap::new();
    for it in &items {
        if let Item::Label(l) = it {
            label_state.insert(*l, ALL);
        }
    }
    let mut converged = false;
    for _ in 0..64 {
        let mut incoming: HashMap<&str, u16> = HashMap::new();
        let mut state: u16 = 0;
        let mut reachable = true;
        for it in &items {
            match it {
                Item::Skip => {}
                Item::Label(l) => {
                    if reachable {
                        let e = incoming.entry(*l).or_insert(ALL);
                        *e &= state;
                    }
                    state = label_state.get(l).copied().unwrap_or(0);
                    reachable = true;
                }
                Item::Instr(t) => {
                    let (m, rest) = match t.split_once(' ') {
                        Some((m, r)) => (m, r),
                        None => (*t, ""),
                    };
                    if let Some(target) = branch_target(m, rest) {
                        match target {
                            Some(name) => {
                                if let Some((k, _)) = label_state.get_key_value(name.as_str()) {
                                    let e = incoming.entry(*k).or_insert(ALL);
                                    *e &= state;
                                }
                            }
                            None => return asm.to_string(),
                        }
                        if m == "jmp" {
                            reachable = false;
                        }
                        continue;
                    }
                    if *t == "ret" {
                        reachable = false;
                        continue;
                    }
                    state = step(state, t);
                }
            }
        }
        let mut next: HashMap<&str, u16> = HashMap::new();
        for k in label_state.keys() {
            next.insert(*k, incoming.get(k).copied().unwrap_or(0));
        }
        if next == label_state {
            converged = true;
            break;
        }
        label_state = next;
    }
    if !converged {
        return asm.to_string();
    }
    let mut res = String::with_capacity(asm.len());
    let mut state: u16 = 0;
    let mut dropped = false;
    for (i, it) in items.iter().enumerate() {
        match it {
            Item::Skip => {}
            Item::Label(l) => {
                state = label_state.get(l).copied().unwrap_or(0);
            }
            Item::Instr(t) => {
                let (m, rest) = match t.split_once(' ') {
                    Some((m, r)) => (m, r),
                    None => (*t, ""),
                };
                if branch_target(m, rest).is_some() || *t == "ret" {
                    if *t == "ret" || m == "jmp" {
                        state = 0;
                    }
                } else if let Some(r) = same_reg32(lines[i]) {
                    if state & (1 << r) != 0 {
                        dropped = true;
                        continue;
                    }
                    state = step(state, t);
                } else {
                    state = step(state, t);
                }
            }
        }
        res.push_str(lines[i]);
        res.push('\n');
    }
    if !dropped {
        return asm.to_string();
    }
    if !asm.ends_with('\n') && res.ends_with('\n') {
        res.pop();
    }
    res
}
