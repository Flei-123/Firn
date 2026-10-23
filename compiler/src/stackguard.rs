// SPDX-License-Identifier: MPL-2.0
//! Stack overflow guard (r904, SPEC 5.2 `L12`): a SIGSEGV whose fault address lies
//! within 1 MiB of the faulting `rsp` ends the program with a message and exit
//! code 101 (the panic code) instead of a silent SIGSEGV 139. Any other SIGSEGV
//! keeps its old behaviour: the handler is one-shot (`SA_RESETHAND`) and returns,
//! the faulting instruction runs again and the default action kills the process.
//! x86-64 Linux user programs only.


pub(crate) const START: &[&str] = &[
    "sub rsp, 64",
    "mov eax, 9",
    "xor edi, edi",
    "mov esi, 65536",
    "mov edx, 3",
    "mov r10d, 34",
    "mov r8, -1",
    "xor r9d, r9d",
    "syscall",
    "mov qword ptr [rsp], rax",
    "mov qword ptr [rsp+8], 0",
    "mov qword ptr [rsp+16], 65536",
    "mov eax, 131",
    "mov rdi, rsp",
    "xor esi, esi",
    "syscall",
    "lea rax, [rip+.L__firn_segv]",
    "mov qword ptr [rsp+32], rax",
    "mov eax, 2348810244",
    "mov qword ptr [rsp+40], rax",
    "lea rax, [rip+.L__firn_sigret]",
    "mov qword ptr [rsp+48], rax",
    "mov qword ptr [rsp+56], 0",
    "mov eax, 13",
    "mov edi, 11",
    "lea rsi, [rsp+32]",
    "xor edx, edx",
    "mov r10d, 8",
    "syscall",
    "add rsp, 64",
];

pub(crate) const TAIL: &[&str] = &[
    ".L__firn_segv:",
    "mov rax, qword ptr [rsi+16]",
    "mov rcx, qword ptr [rdx+160]",
    "sub rax, rcx",
    "add rax, 1048576",
    "cmp rax, 2097152",
    "ja .L__firn_segv_other",
    "mov eax, 1",
    "mov edi, 2",
    "lea rsi, [rip+.L__firn_so_msg]",
    "mov edx, 21",
    "syscall",
    "mov eax, 231",
    "mov edi, 101",
    "syscall",
    ".L__firn_segv_other:",
    "ret",
    ".L__firn_sigret:",
    "mov eax, 15",
    "syscall",
    ".L__firn_so_msg:",
    ".byte 102,105,114,110,58,32,115,116,97,99,107,32,111,118,101,114,102,108,111,119,10",
];

pub(crate) fn enabled() -> bool {
    crate::checkmode::is_checked()
        && !crate::target::windows()
        && !crate::target::shared()
        && !crate::prof::is_kernel()
}

pub(crate) fn emit_start(e: &mut crate::codegen_x86::Emitter) {
    for l in START {
        e.line(l);
    }
}

pub(crate) fn emit_tail(e: &mut crate::codegen_x86::Emitter) {
    for l in TAIL {
        if l.ends_with(':') {
            e.raw(l);
        } else {
            e.line(l);
        }
    }
}
