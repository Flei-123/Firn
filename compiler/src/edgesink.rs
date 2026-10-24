use crate::phisink::{reg_of, regs_in};

const KILLABLE: [usize; 3] = [0, 10, 11];
const SCAN_LIMIT: usize = 12;
const PLAIN: [&str; 16] = [
    "mov", "lea", "movzx", "movsx", "movsxd", "add", "sub", "and", "or", "xor", "cmp", "test", "shl", "shr", "sar", "neg",
];
const WRITE_FIRST: [&str; 5] = ["mov", "lea", "movzx", "movsx", "movsxd"];

fn is_real(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.starts_with(".loc ")
}

fn is_label(t: &str) -> bool {
    t.ends_with(':') && !t.contains(' ')
}

fn plain_reg_pair(t: &str) -> Option<(usize, usize, String, String)> {
    let rest = t.strip_prefix("mov ")?;
    let (d, s) = rest.split_once(", ")?;
    let (di, dw) = reg_of(d)?;
    let (si, sw) = reg_of(s)?;
    if dw != sw || dw > 1 || di == si {
        return None;
    }
    Some((di, si, d.to_string(), s.to_string()))
}

fn dead_on_fallthrough(lines: &[String], from: usize, reg: usize, dest: &str) -> bool {
    let mask = 1u16 << reg;
    let mut seen = 0;
    for line in &lines[from..] {
        let t = line.trim();
        if !is_real(line) || is_label(t) {
            continue;
        }
        seen += 1;
        if seen > SCAN_LIMIT {
            return false;
        }
        if let Some(op) = t.strip_prefix("call ") {
            let target = op.split(' ').next().unwrap_or("");
            return regs_in(target) & mask == 0;
        }
        let (m, rest) = match t.split_once(' ') {
            Some(x) => x,
            None => return false,
        };
        if !PLAIN.contains(&m) {
            return false;
        }
        if regs_in(rest) & mask == 0 {
            continue;
        }
        if WRITE_FIRST.contains(&m) {
            if let Some((d, s)) = rest.split_once(", ") {
                if d == dest && regs_in(s) & mask == 0 {
                    return true;
                }
            }
        }
        return false;
    }
    false
}

fn label_after(lines: &[String], from: usize, target: &str) -> bool {
    let want = format!("{}:", target);
    lines[from..].iter().any(|l| l.trim() == want)
}

pub fn run(asm: &str) -> String {
    if std::env::var_os("FIRN_NO_EDGESINK").is_some() {
        return asm.to_string();
    }
    let mut lines: Vec<String> = asm.lines().map(|l| l.to_string()).collect();
    let mut dropped = vec![false; lines.len()];
    let mut stubs: Vec<String> = Vec::new();
    for j in 0..lines.len() {
        let t = lines[j].trim().to_string();
        if !t.starts_with('j') || t.starts_with("jmp ") {
            continue;
        }
        let (_, target) = match t.rsplit_once(' ') {
            Some(x) => x,
            None => continue,
        };
        if !target.starts_with(".L") || !label_after(&lines, j + 1, target) {
            continue;
        }
        let mut k = j;
        let mut found: Option<(usize, usize, String, String)> = None;
        while k > 0 {
            k -= 1;
            if dropped[k] || !is_real(&lines[k]) {
                continue;
            }
            let u = lines[k].trim();
            if u.starts_with("cmp ") || u.starts_with("test ") {
                continue;
            }
            if let Some((d, _, ds, ss)) = plain_reg_pair(u) {
                found = Some((k, d, ds, ss));
            }
            break;
        }
        let (mi, dreg, dest, src) = match found {
            Some(x) => x,
            None => continue,
        };
        if !KILLABLE.contains(&dreg) {
            continue;
        }
        let src_mask = regs_in(&src);
        let clash = (mi + 1..j).any(|q| {
            let u = lines[q].trim();
            is_real(&lines[q]) && regs_in(u.split_once(' ').map(|x| x.1).unwrap_or("")) & (1u16 << dreg) != 0
        });
        if clash || src_mask == 0 {
            continue;
        }
        if !dead_on_fallthrough(&lines, j + 1, dreg, &dest) {
            continue;
        }
        let stub = format!("{}_es{}", target, j);
        let indent_len = lines[j].len() - lines[j].trim_start().len();
        let head = lines[j][..lines[j].rfind(' ').unwrap()].to_string();
        let moved = lines[mi].trim().to_string();
        lines[j] = format!("{} {}", head, stub);
        dropped[mi] = true;
        let pad = " ".repeat(indent_len);
        stubs.push(format!("{}:", stub));
        stubs.push(format!("{}{}", pad, moved));
        stubs.push(format!("{}jmp {}", pad, target));
    }
    if stubs.is_empty() {
        return asm.to_string();
    }
    let mut res = String::with_capacity(asm.len() + stubs.len() * 24);
    for (i, l) in lines.iter().enumerate() {
        if !dropped[i] {
            res.push_str(l);
            res.push('\n');
        }
    }
    for s in &stubs {
        res.push_str(s);
        res.push('\n');
    }
    res
}
