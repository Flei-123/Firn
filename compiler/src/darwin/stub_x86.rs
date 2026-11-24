// SPDX-License-Identifier: MPL-2.0
use super::syscalls::{open_flags, Entry, Prep, Ret, CLOCK_IDS, ENOSYS, MMAP_FLAGS, STUB_SYMBOL, TABLE, X86_O_DIRECTORY};

pub fn stub_asm() -> String {
    let mut s = String::new();
    s.push_str(".text\n.p2align 4\n");
    s.push_str(&format!(".globl {}\n{}:\n", STUB_SYMBOL, STUB_SYMBOL));
    s.push_str("    push rbp\n    mov rbp, rsp\n    and rsp, -16\n");
    for e in TABLE {
        s.push_str(&format!("    cmp eax, {}\n    je Lfmac_sys{}\n", e.x86, e.x86));
    }
    s.push_str(&format!("    mov rax, -{}\n    jmp Lfmac_ret\n", ENOSYS));
    for e in TABLE {
        s.push_str(&entry_asm(e));
    }
    s.push_str("Lfmac_errno:\n    call ___error\n    movsxd rax, dword ptr [rax]\n    neg rax\n");
    s.push_str("Lfmac_ret:\n    mov rsp, rbp\n    pop rbp\n    ret\n");
    s
}

fn entry_asm(e: &Entry) -> String {
    let mut s = format!("Lfmac_sys{}:\n", e.x86);
    if e.ret == Ret::Zero {
        s.push_str("    xor eax, eax\n    jmp Lfmac_ret\n");
        return s;
    }
    s.push_str(&prep_asm(e.prep));
    s.push_str("    xor eax, eax\n");
    s.push_str(&format!("    call {}\n", e.symbol));
    if e.ret == Ret::Int {
        s.push_str("    movsxd rax, eax\n");
    }
    s.push_str("    cmp rax, -1\n    je Lfmac_errno\n    jmp Lfmac_ret\n");
    s
}

fn prep_asm(prep: Prep) -> String {
    match prep {
        Prep::Open => translate_bits("rsi", &open_flags(X86_O_DIRECTORY), 3) + "    mov rsi, r11\n",
        Prep::Mmap => translate_bits("r10", MMAP_FLAGS, 0x13) + "    mov rcx, r11\n",
        Prep::ClockId => translate_ids(CLOCK_IDS),
        Prep::UnlinkAt | Prep::MkdirAt | Prep::Pipe | Prep::Dup | Prep::None => String::new(),
    }
}

fn translate_bits(src: &str, pairs: &[(u32, u32)], keep: u32) -> String {
    let mut s = format!("    mov r11, {}\n    and r11, {}\n", src, keep);
    for (i, (linux, darwin)) in pairs.iter().enumerate() {
        s.push_str(&format!(
            "    test {}, {}\n    jz Lfmac_f{}_{}\n    or r11, {}\nLfmac_f{}_{}:\n",
            src, linux, src, i, darwin, src, i
        ));
    }
    s
}

fn translate_ids(pairs: &[(u32, u32)]) -> String {
    let mut s = String::new();
    for (i, (linux, darwin)) in pairs.iter().enumerate() {
        s.push_str(&format!(
            "    cmp rdi, {}\n    jne Lfmac_clk{}\n    mov edi, {}\n    jmp Lfmac_clkdone\nLfmac_clk{}:\n",
            linux, i, darwin, i
        ));
    }
    s.push_str("Lfmac_clkdone:\n");
    s
}
