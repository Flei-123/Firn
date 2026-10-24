const CALLEE_SAVED: [&str; 5] = ["rbx", "r12", "r13", "r14", "r15"];

fn is_real(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.starts_with(".loc ")
}

fn mentions_frame(t: &str) -> bool {
    t.contains("rbp")
        || t.contains("rsp")
        || t.contains("ebp")
        || t.contains("esp")
        || t == "leave"
        || t.starts_with("enter ")
        || t.contains("[bp")
        || t.contains(" bp")
}

fn is_call(t: &str) -> bool {
    t.starts_with("call ") || t == "syscall"
}

fn is_header(line: &str) -> bool {
    let t = line.trim();
    !is_real(line) || t.starts_with(".globl ") || (t.ends_with(':') && !t.contains(' '))
}

fn prologue(lines: &[&str]) -> Option<(usize, usize, usize)> {
    let mut idx = 0;
    while idx < lines.len() && is_header(lines[idx]) {
        idx += 1;
    }
    if idx >= lines.len() {
        return None;
    }
    let first = idx;
    if lines[idx].trim() != "push rbp" {
        return None;
    }
    idx += 1;
    while idx < lines.len() && !is_real(lines[idx]) {
        idx += 1;
    }
    if idx >= lines.len() || lines[idx].trim() != "mov rbp, rsp" {
        return None;
    }
    let second = idx;
    idx += 1;
    let mut pushes = 0;
    loop {
        let mut j = idx;
        while j < lines.len() && !is_real(lines[j]) {
            j += 1;
        }
        if j >= lines.len() {
            return Some((first, second, pushes));
        }
        match lines[j].trim().strip_prefix("push ") {
            Some(r) if CALLEE_SAVED.contains(&r) => {
                pushes += 1;
                idx = j + 1;
            }
            _ => return Some((first, second, pushes)),
        }
    }
}

fn own_label(lines: &[&str]) -> Option<String> {
    lines
        .iter()
        .map(|l| l.trim())
        .find(|t| t.ends_with(':') && !t.contains(' ') && !t.starts_with(".L"))
        .map(|t| t.trim_end_matches(':').to_string())
}

fn rewrite_function(lines: &[&str]) -> Option<Vec<String>> {
    let (first, second, pushes) = prologue(lines)?;
    let own = own_label(lines);
    let no_pad_ok = std::env::var_os("FIRN_NO_SELFPAD").is_none();
    let mut has_call = false;
    let mut foreign_call = false;
    let mut pop_rbp: Vec<usize> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == first || i == second || !is_real(line) {
            continue;
        }
        let t = line.trim();
        if t == "pop rbp" {
            pop_rbp.push(i);
            continue;
        }
        if mentions_frame(t) {
            return None;
        }
        if is_call(t) {
            has_call = true;
            let self_call = no_pad_ok && own.as_deref().map(|o| t.strip_prefix("call ").map(|c| c.split(" #").next().unwrap_or(c).trim()) == Some(o)).unwrap_or(false);
            if !self_call {
                foreign_call = true;
            }
        }
    }
    if pop_rbp.is_empty() {
        return None;
    }
    let pad = has_call && foreign_call && pushes % 2 == 0;
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if i == first {
            if pad {
                out.push(line.replace("push rbp", "push rax"));
            }
        } else if i == second {
            continue;
        } else if pop_rbp.binary_search(&i).is_ok() {
            if pad {
                out.push(line.replace("pop rbp", "pop r11"));
            }
        } else {
            out.push(line.to_string());
        }
    }
    Some(out)
}

pub fn run(asm: &str) -> String {
    if std::env::var_os("FIRN_NO_FPOMIT").is_some() {
        return asm.to_string();
    }
    let lines: Vec<&str> = asm.lines().collect();
    match rewrite_function(&lines) {
        Some(v) => {
            let mut res = v.join("\n");
            if asm.ends_with('\n') {
                res.push('\n');
            }
            res
        }
        None => asm.to_string(),
    }
}
