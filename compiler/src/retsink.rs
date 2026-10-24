const CALLER_SAVED: [(&str, &str); 7] = [
    ("rcx", "ecx"),
    ("rdx", "edx"),
    ("rsi", "esi"),
    ("rdi", "edi"),
    ("r8", "r8d"),
    ("r9", "r9d"),
    ("r10", "r10d"),
];

const CALLEE_SAVED: [&str; 5] = ["rbx", "r12", "r13", "r14", "r15"];

fn is_real(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.starts_with(".loc ")
}

fn tail_is_plain_epilogue(lines: &[&str], from: usize, reg: &str) -> bool {
    for line in &lines[from..] {
        let t = line.trim();
        if t.is_empty() || t.starts_with(".loc ") || t == "mov rsp, rbp" || t == "leave" {
            continue;
        }
        if t == "ret" {
            return true;
        }
        if let Some(r) = t.strip_prefix("pop ") {
            if r != reg {
                continue;
            }
            return false;
        }
        if let Some(rest) = t.strip_prefix("mov ") {
            if let Some((d, s)) = rest.split_once(", ") {
                if CALLEE_SAVED.contains(&d) && s.starts_with("qword ptr [rbp-") {
                    continue;
                }
            }
        }
        return false;
    }
    false
}

fn retarget(line: &str, wide: &str, narrow: &str) -> Option<String> {
    let t = line.trim_start();
    let indent = &line[..line.len() - t.len()];
    let (mnemonic, rest) = t.split_once(' ')?;
    if mnemonic != "lea" && mnemonic != "mov" {
        return None;
    }
    let (dst, src) = rest.split_once(", ")?;
    let new_dst = if dst == wide {
        "rax"
    } else if dst == narrow {
        "eax"
    } else {
        return None;
    };
    if mnemonic == "mov" && src.contains(new_dst) {
        return None;
    }
    Some(format!("{}{} {}, {}", indent, mnemonic, new_dst, src))
}

pub fn run(asm: &str) -> String {
    if std::env::var_os("FIRN_NO_RETSINK").is_some() {
        return asm.to_string();
    }
    let lines: Vec<&str> = asm.lines().collect();
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    let mut dropped = vec![false; lines.len()];
    let mut prev: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        if !is_real(line) {
            continue;
        }
        let t = line.trim();
        if let (Some(p), Some(rest)) = (prev, t.strip_prefix("mov rax, ")) {
            if let Some((wide, narrow)) = CALLER_SAVED.iter().find(|(w, _)| *w == rest) {
                if tail_is_plain_epilogue(&lines, i + 1, wide) {
                    if let Some(new_line) = retarget(lines[p], wide, narrow) {
                        out[p] = new_line;
                        dropped[i] = true;
                    }
                }
            }
        }
        prev = Some(i);
    }
    let mut res = String::with_capacity(asm.len());
    for (i, l) in out.iter().enumerate() {
        if !dropped[i] {
            res.push_str(l);
            res.push('\n');
        }
    }
    if !asm.ends_with('\n') && res.ends_with('\n') {
        res.pop();
    }
    res
}
