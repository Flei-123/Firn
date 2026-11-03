use super::liveness::{flags_dead_after, label_map, reg_dead_after};

fn query(text: &str, reg: usize) -> (bool, bool) {
    let lines: Vec<&str> = text.lines().collect();
    let labels = label_map(&lines);
    (reg_dead_after(&lines, 0, reg, &labels), flags_dead_after(&lines, 0, &labels))
}

#[test]
fn register_dead_when_every_successor_overwrites_it() {
    let t = "    jne .La\n    mov rcx, 1\n    ret\n.La:\n    xor ecx, ecx\n    cmp rax, 1\n    ret";
    assert_eq!(query(t, 1), (true, false));
}

#[test]
fn register_live_when_one_successor_reads_it() {
    let t = "    jmp .Lb\n.La:\n    mov rcx, 1\n    ret\n.Lb:\n    add rax, rcx\n    jmp .La";
    assert!(!query(t, 1).0);
}

#[test]
fn loops_terminate_and_stay_conservative() {
    let t = ".Lh:\n    add rax, 1\n    cmp rax, 9\n    jne .Lh\n    ret";
    assert_eq!(query(t, 0), (false, true));
    assert_eq!(query(t, 1), (true, true));
}

#[test]
fn unknown_targets_and_calls_keep_everything_live() {
    assert_eq!(query("    jc .Lpanic_elsewhere\n    ret", 1), (false, false));
    assert_eq!(query("    call foo\n    mov rcx, 1\n    ret", 1), (false, false));
    assert_eq!(query("    jmp rax", 1), (false, false));
}

#[test]
fn return_keeps_result_and_callee_saved_registers() {
    assert!(!query("    ret", 0).0);
    assert!(!query("    ret", 3).0);
    assert!(query("    ret", 1).0);
    assert!(query("    ret", 11).0);
}

#[test]
fn data_directives_are_barriers() {
    assert_eq!(query("    .quad 0\n    ret", 1), (false, false));
}
