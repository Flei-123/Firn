mod liveness;
mod normalize;
mod rules;
#[cfg(test)]
mod rule_tests;
#[cfg(test)]
mod liveness_tests;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::OnceLock;

use liveness::Labels;

pub(crate) use rules::{Rule, RULES};

const MAX_WINDOW: usize = 6;
const MAX_ROUNDS: usize = 4;

fn table() -> &'static HashMap<String, Vec<&'static Rule>> {
    static TABLE: OnceLock<HashMap<String, Vec<&'static Rule>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut m: HashMap<String, Vec<&'static Rule>> = HashMap::new();
        for r in RULES {
            m.entry(r.pattern.join("\n")).or_default().push(r);
        }
        m
    })
}

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("FIRN_NO_SUPEROPT").map_or(true, |v| v.is_empty() || v == "0"))
}

fn applies(rule: &Rule, lines: &[&str], end: usize, regs: &[usize], labels: &Labels) -> bool {
    if rule.flags_dead && !liveness::flags_dead_after(lines, end, labels) {
        return false;
    }
    (0..regs.len()).all(|v| rule.dead >> v & 1 == 0 || liveness::reg_dead_after(lines, end, regs[v], labels))
}

fn rewrite_at(lines: &[&str], i: usize, labels: &Labels, out: &mut Vec<String>) -> Option<usize> {
    let tab = table();
    let windows = normalize::prefixes(&lines[i..], MAX_WINDOW);
    for (len, key, regs) in windows.into_iter().rev() {
        if len < 1 {
            continue;
        }
        let Some(cands) = tab.get(&key) else { continue };
        for rule in cands {
            if !applies(rule, lines, i + len, &regs, labels) {
                continue;
            }
            let rendered: Option<Vec<String>> = rule.replacement.iter().map(|t| normalize::render(t, &regs)).collect();
            let Some(rendered) = rendered else { continue };
            out.extend(rendered.into_iter().map(|l| format!("    {}", l)));
            return Some(len);
        }
    }
    None
}

fn round(asm: &str) -> (String, usize) {
    let lines: Vec<&str> = asm.lines().collect();
    let labels = liveness::label_map(&lines);
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut hits = 0;
    let mut i = 0;
    while i < lines.len() {
        if normalize::eligible(lines[i]) {
            if let Some(n) = rewrite_at(&lines, i, &labels, &mut out) {
                hits += 1;
                i += n;
                continue;
            }
        }
        out.push(lines[i].to_string());
        i += 1;
    }
    let mut text = out.join("\n");
    if asm.ends_with('\n') {
        text.push('\n');
    }
    (text, hits)
}

pub(crate) fn run(asm: &str) -> String {
    if RULES.is_empty() || !enabled() {
        return asm.to_string();
    }
    let mut text = asm.to_string();
    for _ in 0..MAX_ROUNDS {
        let (next, hits) = round(&text);
        text = next;
        if hits == 0 {
            break;
        }
    }
    text
}
