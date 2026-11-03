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

fn is_boundary(s: &str) -> bool {
    s.ends_with(':') || (s.starts_with('.') && !s.starts_with(".loc"))
}

fn skip(s: &str) -> bool {
    s.is_empty() || s.starts_with('#') || s.starts_with(".loc")
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

pub(crate) fn reg_dead_after(lines: &[&str], start: usize, reg: usize) -> bool {
    for line in &lines[start.min(lines.len())..] {
        let s = line.trim();
        if skip(s) {
            continue;
        }
        if is_boundary(s) {
            return false;
        }
        let Some((mn, ops)) = split_line(s) else { continue };
        if !EXPLICIT.contains(&mn) || (mn == "imul" && ops.len() == 1) {
            return false;
        }
        if ops.is_empty() {
            continue;
        }
        let rest: Vec<usize> = ops[1..].iter().flat_map(|o| regs_in(o)).collect();
        if mn == "xor" && ops.len() == 2 && ops[0] == ops[1] && full_write_of(ops[0], reg) {
            return true;
        }
        if PURE_WRITE.contains(&mn) && full_write_of(ops[0], reg) && !rest.contains(&reg) {
            return true;
        }
        if rest.contains(&reg) || regs_in(ops[0]).contains(&reg) {
            return false;
        }
    }
    false
}

pub(crate) fn flags_dead_after(lines: &[&str], start: usize) -> bool {
    for line in &lines[start.min(lines.len())..] {
        let s = line.trim();
        if skip(s) {
            continue;
        }
        if is_boundary(s) {
            return false;
        }
        let Some((mn, _)) = split_line(s) else { continue };
        if FLAG_FULL.contains(&mn) {
            return true;
        }
        if !FLAG_NEUTRAL.contains(&mn) {
            return false;
        }
    }
    false
}
