use std::collections::{HashMap, HashSet};

use super::normalize::{parse_reg, split_line};

const PURE_WRITE: [&str; 5] = ["mov", "movzx", "movsx", "movsxd", "lea"];

const EXPLICIT: [&str; 54] = [
    "mov", "movzx", "movsx", "movsxd", "lea", "add", "sub", "and", "or", "xor", "cmp", "test", "imul", "shl",
    "shr", "sar", "rol", "ror", "neg", "not", "inc", "dec", "movss", "movsd", "movq", "movd", "cvtsi2sd",
    "cvtsi2ss", "cvttsd2si", "cvttss2si", "addsd", "subsd", "mulsd", "divsd", "addss", "subss", "mulss",
    "divss", "ucomisd", "ucomiss", "movaps", "movapd", "xorps", "xorpd", "andps", "andpd", "sqrtsd",
    "sqrtss", "popcnt", "lzcnt", "tzcnt", "bswap", "nop", "endbr64",
];

const FLAG_FULL: [&str; 8] = ["add", "sub", "and", "or", "xor", "cmp", "test", "neg"];

const FLAG_NEUTRAL: [&str; 14] = [
    "mov", "movzx", "movsx", "movsxd", "lea", "not", "nop", "movss", "movsd", "movq", "movd", "movaps",
    "movapd", "bswap",
];

const JCC: [&str; 30] = [
    "jo", "jno", "jb", "jc", "jnae", "jae", "jnb", "jnc", "je", "jz", "jne", "jnz", "jbe", "jna", "ja", "jnbe",
    "js", "jns", "jp", "jpe", "jnp", "jpo", "jl", "jnge", "jge", "jnl", "jle", "jng", "jg", "jnle",
];

const QUIET_DIRECTIVES: [&str; 4] = [".loc", ".p2align", ".align", ".cfi_"];

const LIVE_AT_RET: [usize; 9] = [0, 2, 3, 4, 5, 12, 13, 14, 15];

const STEP_BUDGET: i64 = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Dead,
    Live,
    Next,
    Cut,
}

enum Kind<'a> {
    Skip,
    Barrier,
    Insn(&'a str, Vec<&'a str>),
}

pub(crate) type Labels<'a> = HashMap<&'a str, usize>;

pub(crate) fn label_map<'a>(lines: &[&'a str]) -> Labels<'a> {
    let mut out = HashMap::new();
    for (i, line) in lines.iter().enumerate() {
        let s = line.trim();
        if s.ends_with(':') && !s.contains(' ') && !s.contains('\t') {
            out.entry(&s[..s.len() - 1]).or_insert(i);
        }
    }
    out
}

fn classify(s: &str) -> Kind<'_> {
    if s.is_empty() || s.starts_with('#') || QUIET_DIRECTIVES.iter().any(|d| s.starts_with(d)) {
        return Kind::Skip;
    }
    if s.ends_with(':') {
        return Kind::Skip;
    }
    if s.starts_with('.') {
        return Kind::Barrier;
    }
    match split_line(s) {
        Some((mn, ops)) => Kind::Insn(mn, ops),
        None => Kind::Skip,
    }
}

fn regs_in(op: &str) -> Vec<usize> {
    op.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$'))
        .filter(|t| t.chars().next().map_or(false, |c| !c.is_ascii_digit()))
        .filter_map(|t| parse_reg(t).map(|r| r.0))
        .collect()
}

fn full_write_of(op: &str, reg: usize) -> bool {
    matches!(parse_reg(op), Some((r, w)) if r == reg && w <= 1)
}

fn control(mn: &str, ops: &[&str], labels: &Labels, stack: &mut Vec<usize>) -> Option<Step> {
    if mn == "ret" {
        return Some(Step::Cut);
    }
    let jcc = JCC.contains(&mn);
    if mn != "jmp" && !jcc {
        return None;
    }
    match (ops.len(), ops.first().and_then(|t| labels.get(t))) {
        (1, Some(&target)) => {
            stack.push(target);
            Some(if jcc { Step::Next } else { Step::Cut })
        }
        _ => Some(Step::Live),
    }
}

fn reg_step(mn: &str, ops: &[&str], reg: usize) -> Step {
    if mn == "ret" {
        return if LIVE_AT_RET.contains(&reg) { Step::Live } else { Step::Dead };
    }
    if mn == "jmp" || JCC.contains(&mn) {
        return Step::Next;
    }
    if !EXPLICIT.contains(&mn) || (mn == "imul" && ops.len() == 1) {
        return Step::Live;
    }
    if ops.is_empty() {
        return Step::Next;
    }
    let rest: Vec<usize> = ops[1..].iter().flat_map(|o| regs_in(o)).collect();
    if mn == "xor" && ops.len() == 2 && ops[0] == ops[1] && full_write_of(ops[0], reg) {
        return Step::Dead;
    }
    if PURE_WRITE.contains(&mn) && full_write_of(ops[0], reg) && !rest.contains(&reg) {
        return Step::Dead;
    }
    if rest.contains(&reg) || regs_in(ops[0]).contains(&reg) {
        return Step::Live;
    }
    Step::Next
}

fn flags_step(mn: &str, _ops: &[&str]) -> Step {
    if mn == "ret" || FLAG_FULL.contains(&mn) {
        return Step::Dead;
    }
    if JCC.contains(&mn) {
        return Step::Live;
    }
    if FLAG_NEUTRAL.contains(&mn) || mn == "jmp" {
        return Step::Next;
    }
    Step::Live
}

fn dead_after(lines: &[&str], start: usize, labels: &Labels, step: &dyn Fn(&str, &[&str]) -> Step) -> bool {
    let mut stack = vec![start];
    let mut seen: HashSet<usize> = HashSet::new();
    let mut budget = STEP_BUDGET;
    while let Some(mut i) = stack.pop() {
        if !seen.insert(i) {
            continue;
        }
        loop {
            if i >= lines.len() {
                return false;
            }
            budget -= 1;
            if budget < 0 {
                return false;
            }
            let kind = classify(lines[i].trim());
            i += 1;
            let (mn, ops) = match kind {
                Kind::Skip => continue,
                Kind::Barrier => return false,
                Kind::Insn(mn, ops) => (mn, ops),
            };
            match step(mn, &ops) {
                Step::Live => return false,
                Step::Dead => break,
                _ => {}
            }
            match control(mn, &ops, labels, &mut stack) {
                Some(Step::Live) => return false,
                Some(Step::Cut) => break,
                _ => {}
            }
        }
    }
    true
}

pub(crate) fn reg_dead_after(lines: &[&str], start: usize, reg: usize, labels: &Labels) -> bool {
    dead_after(lines, start, labels, &|mn, ops| reg_step(mn, ops, reg))
}

pub(crate) fn flags_dead_after(lines: &[&str], start: usize, labels: &Labels) -> bool {
    dead_after(lines, start, labels, &flags_step)
}
