const RAX: u16 = 1;

const NAMES: [[&str; 4]; 16] = [
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

const CANDIDATES: [usize; 8] = [1, 2, 6, 7, 8, 9, 10, 11];
const ARG_MASK: u16 = (1 << 7) | (1 << 6) | (1 << 2) | (1 << 1) | (1 << 8) | (1 << 9);
const CALLER_MASK: u16 = ARG_MASK | RAX | (1 << 10) | (1 << 11);

const PURE_DEF: [&str; 12] = [
    "mov", "movzx", "movsx", "movsxd", "lea", "movd", "movq", "cvttsd2si", "cvttss2si", "popcnt", "lzcnt", "tzcnt",
];

const READ_ONLY: [&str; 12] = [
    "cmp", "test", "push", "bt", "ucomisd", "ucomiss", "comisd", "comiss", "nop", "pause", "lfence", "prefetcht0",
];

const RMW: [&str; 24] = [
    "add", "sub", "and", "or", "xor", "adc", "sbb", "shl", "shr", "sar", "rol", "ror", "neg", "not", "inc", "dec",
    "imul", "bswap", "cmove", "cmovne", "cmovl", "cmovg", "cmovb", "cmova",
];

pub(crate) fn reg_of(tok: &str) -> Option<(usize, usize)> {
    for (i, row) in NAMES.iter().enumerate() {
        for (w, n) in row.iter().enumerate() {
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

pub(crate) fn regs_in(s: &str) -> u16 {
    let mut mask = 0u16;
    for tok in s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if let Some((i, _)) = reg_of(tok) {
            mask |= 1 << i;
        }
    }
    mask
}

#[derive(Clone, Copy, Default)]
struct Touch {
    reads: u16,
    writes: u16,
    unknown: bool,
    implicit: bool,
}

fn touch(t: &str) -> Touch {
    let (m, rest) = match t.split_once(' ') {
        Some((m, r)) => (m, r),
        None => (t, ""),
    };
    let mut x = Touch::default();
    match m {
        "call" => {
            x.reads = match rest.split_once(" #") {
                Some((target, marks)) => regs_in(target) | regs_in(marks),
                None => ARG_MASK | regs_in(rest),
            };
            x.writes = CALLER_MASK;
            x.implicit = true;
            return x;
        }
        "syscall" => {
            x.reads = ARG_MASK | RAX | (1 << 10);
            x.writes = RAX | (1 << 1) | (1 << 11);
            x.implicit = true;
            return x;
        }
        "ret" => {
            x.reads = RAX | (1 << 2);
            x.implicit = true;
            return x;
        }
        "pop" => {
            if let Some((i, 0)) = reg_of(rest.trim()) {
                x.writes = 1 << i;
            } else {
                x.unknown = true;
            }
            return x;
        }
        _ => {}
    }
    let ops: Vec<&str> = rest.split(", ").map(|s| s.trim()).collect();
    let first = ops.first().copied().unwrap_or("");
    let dest = reg_of(first);
    if m == "xor" || m == "sub" {
        if ops.len() == 2 && ops[0] == ops[1] {
            if let Some((i, w)) = dest {
                if w <= 1 {
                    x.writes = 1 << i;
                    return x;
                }
            }
        }
    }
    if PURE_DEF.contains(&m) || (m == "imul" && ops.len() == 3) {
        match dest {
            Some((i, w)) if w <= 1 => {
                x.writes = 1 << i;
                x.reads = regs_in(&ops[1..].join(", "));
            }
            Some((i, _)) => {
                x.reads = regs_in(rest) | (1 << i);
            }
            None => {
                x.reads = regs_in(rest);
            }
        }
        return x;
    }
    if READ_ONLY.contains(&m) || RMW.contains(&m) || (m.starts_with("set") && m.len() <= 6) {
        x.reads = regs_in(rest);
        return x;
    }
    if (first.starts_with("xmm") || first.starts_with("ymm")) && !m.starts_with("pcmp") && m != "maskmovdqu" {
        x.reads = regs_in(rest);
        return x;
    }
    x.unknown = true;
    x
}

#[derive(Clone, PartialEq)]
enum Kind {
    Skip,
    Label(String),
    Branch { target: String, cond: bool },
    Instr,
}

struct Node {
    text: String,
    kind: Kind,
}

fn classify(line: &str) -> Option<Kind> {
    let t = line.trim();
    if t.is_empty() || t.starts_with(".loc ") || t.starts_with(".globl ") || t.starts_with(".p2align") || t.starts_with(".balign") || t.starts_with('#') {
        return Some(Kind::Skip);
    }
    if t.starts_with('.') && !t.ends_with(':') {
        return None;
    }
    if t.ends_with(':') && !t.contains(' ') {
        return Some(Kind::Label(t[..t.len() - 1].to_string()));
    }
    if t.starts_with('j') {
        let (m, rest) = t.split_once(' ')?;
        let target = rest.trim();
        if !(target.starts_with('.') || target.starts_with('_')) {
            return None;
        }
        return Some(Kind::Branch { target: target.to_string(), cond: m != "jmp" });
    }
    if t.contains(".L") {
        return None;
    }
    Some(Kind::Instr)
}

struct Graph {
    nodes: Vec<Node>,
    succ: Vec<Vec<usize>>,
    pred: Vec<Vec<usize>>,
}

fn build(lines: &[&str]) -> Option<Graph> {
    let mut nodes = Vec::with_capacity(lines.len());
    for l in lines {
        nodes.push(Node { text: l.trim().to_string(), kind: classify(l)? });
    }
    let n = nodes.len();
    let mut label_at = std::collections::HashMap::new();
    for (i, nd) in nodes.iter().enumerate() {
        if let Kind::Label(name) = &nd.kind {
            label_at.insert(name.clone(), i);
        }
    }
    let mut succ = vec![Vec::new(); n];
    let mut pred = vec![Vec::new(); n];
    for i in 0..n {
        let mut s = Vec::new();
        match &nodes[i].kind {
            Kind::Branch { target, cond } => {
                if let Some(&t) = label_at.get(target) {
                    s.push(t);
                }
                if *cond && i + 1 < n {
                    s.push(i + 1);
                }
            }
            _ => {
                if nodes[i].text == "ret" {
                } else if i + 1 < n {
                    s.push(i + 1);
                }
            }
        }
        for &t in &s {
            pred[t].push(i);
        }
        succ[i] = s;
    }
    Some(Graph { nodes, succ, pred })
}

fn node_touch(g: &Graph, i: usize) -> Touch {
    match g.nodes[i].kind {
        Kind::Instr => touch(&g.nodes[i].text),
        _ => Touch::default(),
    }
}

fn rename(text: &str, from: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut tok = String::new();
    let flush = |tok: &mut String, out: &mut String| {
        if !tok.is_empty() {
            let mut done = false;
            for (w, n) in NAMES[from].iter().enumerate() {
                if tok == n {
                    out.push_str(NAMES[0][w]);
                    done = true;
                    break;
                }
            }
            if !done {
                out.push_str(tok);
            }
            tok.clear();
        }
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            tok.push(c);
        } else {
            flush(&mut tok, &mut out);
            out.push(c);
        }
    }
    flush(&mut tok, &mut out);
    out
}

fn writes_dest(g: &Graph, i: usize, r: usize) -> bool {
    let text = &g.nodes[i].text;
    let (m, rest) = match text.split_once(' ') {
        Some(x) => x,
        None => return false,
    };
    if READ_ONLY.contains(&m) {
        return false;
    }
    match rest.split(", ").next().and_then(|f| reg_of(f.trim())) {
        Some((d, _)) => d == r,
        None => false,
    }
}

fn rax_dead_after(g: &Graph, from: usize) -> bool {
    let mut seen = vec![false; g.nodes.len()];
    let mut stack: Vec<usize> = g.succ[from].clone();
    while let Some(i) = stack.pop() {
        if seen[i] {
            continue;
        }
        seen[i] = true;
        let t = node_touch(g, i);
        if t.unknown || t.reads & RAX != 0 {
            return false;
        }
        if t.writes & RAX != 0 {
            continue;
        }
        stack.extend(g.succ[i].iter().copied());
    }
    true
}

fn try_candidate(g: &Graph, j: usize, r: usize) -> Option<(Vec<usize>, Vec<usize>)> {
    let bit = 1u16 << r;
    let n = g.nodes.len();
    let mut defs: Vec<usize> = vec![j];
    let mut uses: Vec<usize> = Vec::new();
    let mut fwd = vec![false; n];
    let mut bwd = vec![false; n];
    for _round in 0..32 {
        let before = (defs.len(), uses.len());
        for f in fwd.iter_mut() {
            *f = false;
        }
        let mut stack: Vec<usize> = defs.iter().flat_map(|&d| g.succ[d].clone()).collect();
        while let Some(i) = stack.pop() {
            if fwd[i] {
                continue;
            }
            fwd[i] = true;
            let t = node_touch(g, i);
            if t.unknown {
                return None;
            }
            if t.reads & bit != 0 {
                if t.implicit || !matches!(g.nodes[i].kind, Kind::Instr) {
                    return None;
                }
                if !uses.contains(&i) {
                    uses.push(i);
                }
            } else if t.writes & bit != 0 {
                continue;
            }
            stack.extend(g.succ[i].iter().copied());
        }
        for b in bwd.iter_mut() {
            *b = false;
        }
        let mut stack: Vec<usize> = uses.iter().flat_map(|&u| g.pred[u].clone()).collect();
        if uses.is_empty() {
            return None;
        }
        while let Some(i) = stack.pop() {
            if bwd[i] {
                continue;
            }
            bwd[i] = true;
            let t = node_touch(g, i);
            if t.unknown {
                return None;
            }
            if t.reads & bit == 0 && t.writes & bit != 0 {
                if !defs.contains(&i) {
                    defs.push(i);
                }
                continue;
            }
            if g.pred[i].is_empty() {
                return None;
            }
            stack.extend(g.pred[i].iter().copied());
        }
        for &u in &uses {
            if g.pred[u].is_empty() {
                return None;
            }
        }
        if (defs.len(), uses.len()) == before {
            break;
        }
        if _round == 31 {
            return None;
        }
    }
    for &d in &defs {
        if d == j {
            continue;
        }
        let t = node_touch(g, d);
        let mn = g.nodes[d].text.split(' ').next().unwrap_or("");
        if !PURE_DEF.contains(&mn) || t.reads & RAX != 0 || t.unknown || t.implicit {
            return None;
        }
    }
    let dt = node_touch(g, j);
    if dt.reads & !RAX != 0 {
        return None;
    }
    for i in 0..n {
        if fwd[i] && bwd[i] {
            let t = node_touch(g, i);
            if (t.reads | t.writes) & RAX != 0 || t.unknown {
                return None;
            }
        }
    }
    for &u in &uses {
        let t = node_touch(g, u);
        if (t.reads | t.writes) & RAX != 0 {
            return None;
        }
    }
    let rewritten: Vec<usize> = defs
        .iter()
        .copied()
        .filter(|&d| d != j)
        .chain(uses.iter().copied().filter(|&u| writes_dest(g, u, r)))
        .collect();
    for d in rewritten {
        if !rax_dead_after(g, d) {
            return None;
        }
    }
    Some((defs, uses))
}

pub fn run(asm: &str) -> String {
    if std::env::var_os("FIRN_NO_PHISINK").is_some() {
        return asm.to_string();
    }
    if !asm.contains("rax") {
        return asm.to_string();
    }
    let mut lines: Vec<String> = asm.lines().map(|s| s.to_string()).collect();
    let mut changed = false;
    for _ in 0..64 {
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let g = match build(&refs) {
            Some(g) => g,
            None => break,
        };
        let mut applied = None;
        'outer: for j in 0..g.nodes.len() {
            if g.nodes[j].kind != Kind::Instr {
                continue;
            }
            let t = &g.nodes[j].text;
            let rest = match t.strip_prefix("mov ") {
                Some(r) => r,
                None => continue,
            };
            let (d, s) = match rest.split_once(", ") {
                Some(x) => x,
                None => continue,
            };
            if s != "rax" {
                continue;
            }
            let r = match reg_of(d) {
                Some((i, 0)) if CANDIDATES.contains(&i) => i,
                _ => continue,
            };
            if let Some((defs, uses)) = try_candidate(&g, j, r) {
                applied = Some((j, r, defs, uses));
                break 'outer;
            }
        }
        match applied {
            None => break,
            Some((j, r, defs, uses)) => {
                let mut rewrites: Vec<(usize, String)> = Vec::new();
                for &d in defs.iter().chain(uses.iter()) {
                    if d == j {
                        continue;
                    }
                    rewrites.push((d, rename(lines[d].trim(), r)));
                }
                for (i, text) in rewrites {
                    let indent_len = lines[i].len() - lines[i].trim_start().len();
                    let indent = lines[i][..indent_len].to_string();
                    lines[i] = format!("{}{}", indent, text);
                }
                lines.remove(j);
                changed = true;
            }
        }
    }
    if !changed {
        return asm.to_string();
    }
    let mut res = lines.join("\n");
    if asm.ends_with('\n') {
        res.push('\n');
    }
    res
}

pub fn strip_marks(asm: &str) -> String {
    if !asm.contains(" #") {
        return asm.to_string();
    }
    let mut res = String::with_capacity(asm.len());
    for l in asm.lines() {
        if l.trim_start().starts_with("call ") {
            if let Some(p) = l.find(" #") {
                res.push_str(&l[..p]);
                res.push('\n');
                continue;
            }
        }
        res.push_str(l);
        res.push('\n');
    }
    if !asm.ends_with('\n') && res.ends_with('\n') {
        res.pop();
    }
    res
}
