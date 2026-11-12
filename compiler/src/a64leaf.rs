// SPDX-License-Identifier: GPL-2.0-only
const PROLOGUE_PAIR: &str = "stp x29, x30, [sp, #-16]!";
const PROLOGUE_FRAME_POINTER: &str = "mov x29, sp";
const EPILOGUE_PAIR: &str = "ldp x29, x30, [sp], #16";
const FRAME_REGISTER_TOKENS: [&str; 7] = ["sp", "x29", "x30", "w29", "w30", "lr", "fp"];

fn is_directive_or_comment(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with('.') || trimmed.starts_with("//")
}

fn real_lines(lines: &[&str], from: usize, to: usize) -> Vec<usize> {
    (from..to)
        .filter(|&i| {
            let t = lines[i].trim();
            !is_directive_or_comment(t) && !t.ends_with(':')
        })
        .collect()
}

fn mentions_frame_register(trimmed: &str) -> bool {
    trimmed
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| FRAME_REGISTER_TOKENS.contains(&token))
}

fn frame_amount(trimmed: &str, mnemonic: &str) -> Option<u64> {
    trimmed.strip_prefix(mnemonic)?.strip_prefix(" sp, sp, #")?.parse().ok()
}

fn removable_frame_lines(lines: &[&str], start: usize, end: usize) -> Option<Vec<usize>> {
    let real = real_lines(lines, start, end);
    if real.len() < 2 || lines[real[0]].trim() != PROLOGUE_PAIR || lines[real[1]].trim() != PROLOGUE_FRAME_POINTER {
        return None;
    }
    let mut remove = vec![real[0], real[1]];
    let mut first_body = 2;
    let mut reserve = None;
    if let Some(&idx) = real.get(2) {
        if let Some(n) = frame_amount(lines[idx].trim(), "sub") {
            remove.push(idx);
            reserve = Some(n);
            first_body = 3;
        }
    }
    for (pos, &idx) in real.iter().enumerate().skip(first_body) {
        let t = lines[idx].trim();
        if t.starts_with("bl ") || t.starts_with("blr ") {
            return None;
        }
        if t == EPILOGUE_PAIR {
            if real.get(pos + 1).map(|&j| lines[j].trim()) != Some("ret") {
                return None;
            }
            remove.push(idx);
            continue;
        }
        if let (Some(n), Some(&next)) = (frame_amount(t, "add"), real.get(pos + 1)) {
            if reserve == Some(n) && lines[next].trim() == EPILOGUE_PAIR {
                remove.push(idx);
                continue;
            }
        }
        if mentions_frame_register(t) {
            return None;
        }
    }
    Some(remove)
}

fn function_starts(lines: &[&str]) -> Vec<usize> {
    (0..lines.len()).filter(|&i| lines[i].starts_with(".globl ")).collect()
}

pub(crate) fn strip_leaf_frames(text: &str) -> String {
    if std::env::var_os("FIRN_NO_A64_LEAF").is_some() {
        return text.to_string();
    }
    let lines: Vec<&str> = text.lines().collect();
    let starts = function_starts(&lines);
    let mut drop = vec![false; lines.len()];
    for (k, &start) in starts.iter().enumerate() {
        let end = starts.get(k + 1).copied().unwrap_or(lines.len());
        if let Some(remove) = removable_frame_lines(&lines, start, end) {
            for i in remove {
                drop[i] = true;
            }
        }
    }
    let mut out = String::with_capacity(text.len());
    for (i, line) in lines.iter().enumerate() {
        if !drop[i] {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_leaf_frames;

    const LEAF: &str = ".globl f\nf:\n    stp x29, x30, [sp, #-16]!\n    mov x29, sp\n    sub sp, sp, #16\n    add x0, x0, x1\n    add sp, sp, #16\n    ldp x29, x30, [sp], #16\n    ret\n";

    #[test]
    fn leaf_without_frame_use_loses_prologue_and_epilogue() {
        assert_eq!(strip_leaf_frames(LEAF), ".globl f\nf:\n    add x0, x0, x1\n    ret\n");
    }

    #[test]
    fn function_with_call_keeps_frame() {
        let text = LEAF.replace("add x0, x0, x1", "bl g");
        assert_eq!(strip_leaf_frames(&text), text);
    }

    #[test]
    fn function_with_slot_access_keeps_frame() {
        let text = LEAF.replace("add x0, x0, x1", "ldr x0, [sp, #8]");
        assert_eq!(strip_leaf_frames(&text), text);
    }
}
