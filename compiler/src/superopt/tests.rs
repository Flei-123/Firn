use super::normalize::{normalize, render, GPR};
use super::{run, Rule, RULES};

const TEST_REGS: [usize; 8] = [0, 1, 2, 6, 7, 8, 9, 10];
const FLAG_KILL: &str = "    test r11, r11";
const FENCE: &str = "    .loc 1 1 0";

fn body(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|t| format!("    {}", render(t, &TEST_REGS).unwrap())).collect()
}

fn tail(rule: &Rule, nvars: usize) -> Vec<String> {
    let mut out = vec![FENCE.to_string()];
    for v in 0..nvars {
        if rule.dead >> v & 1 == 1 {
            out.push(format!("    mov {}, 0", GPR[TEST_REGS[v]][1]));
        }
    }
    if rule.flags_dead {
        out.push(FLAG_KILL.to_string());
    }
    out.push("    ret".to_string());
    out
}

fn nvars(rule: &Rule) -> usize {
    let text: Vec<String> = body(rule.pattern);
    let refs: Vec<&str> = text.iter().map(|s| s.as_str()).collect();
    normalize(&refs).unwrap().1.len()
}

fn weaker(a: &Rule, b: &Rule) -> bool {
    a.pattern == b.pattern && a.dead & !b.dead == 0 && (!a.flags_dead || b.flags_dead)
}

pub(super) fn check_rule(i: usize) {
    let rule = &RULES[i];
    let n = nvars(rule);
    let pat = body(rule.pattern);
    let refs: Vec<&str> = pat.iter().map(|s| s.as_str()).collect();
    assert_eq!(normalize(&refs).unwrap().0, rule.pattern.join("\n"), "pattern of rule {} is not canonical", i);
    assert!(rule.replacement.len() < rule.pattern.len(), "rule {} does not shrink", i);
    let first = RULES.iter().find(|r| weaker(r, rule)).unwrap();
    let mut input = pat.clone();
    input.extend(tail(rule, n));
    let mut expected = body(first.replacement);
    expected.extend(tail(rule, n));
    assert_eq!(run(&input.join("\n")), expected.join("\n"), "rule {}", i);
    if RULES.iter().any(|r| r.pattern == rule.pattern && r.dead == 0 && !r.flags_dead) {
        return;
    }
    let mut live = pat.clone();
    live.push("    jo .Lpanic".to_string());
    live.push("    ret".to_string());
    assert_eq!(run(&live.join("\n")), live.join("\n"), "rule {} must not fire with live values", i);
}

#[test]
fn every_rule_has_a_generated_test() {
    let src = include_str!("rule_tests.rs");
    assert_eq!(src.matches("#[test]").count(), RULES.len());
}

#[test]
fn flags_read_by_an_overflow_check_block_the_rewrite() {
    let text = "    mov rax, rcx\n    add rax, rdx\n    jo .Lpanic\n    ret";
    assert_eq!(run(text), text);
}

#[test]
fn labels_break_windows() {
    let text = "    mov rax, rcx\n.L1:\n    add rax, rdx\n    test r11, r11\n    ret";
    assert_eq!(run(text), text);
}

#[test]
fn stack_registers_are_never_matched() {
    let text = "    mov rbp, rsp\n    add rbp, rdx\n    test r11, r11\n    ret";
    assert_eq!(run(text), text);
}

#[test]
fn normalization_numbers_registers_by_first_use() {
    let (key, regs) = normalize(&["    mov r9d, edi", "    lea rdi, [r9+r9*4]"]).unwrap();
    assert_eq!(key, "mov %0d, %1d\nlea %1, [%0+%0*4]");
    assert_eq!(regs, vec![9, 7]);
}
