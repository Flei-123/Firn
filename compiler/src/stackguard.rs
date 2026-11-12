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

pub(crate) const THREAD_ALT: &[&str] = &[
    "lea rax, [rsi+16]",
    "sub rsp, 32",
    "mov qword ptr [rsp], rax",
    "mov qword ptr [rsp+8], 0",
    "mov qword ptr [rsp+16], 32768",
    "mov eax, 131",
    "mov rdi, rsp",
    "xor esi, esi",
    "syscall",
    "add rsp, 32",
];

pub(crate) const TAIL: &[&str] = &[
    ".L__firn_segv:",
    "mov rax, qword ptr [rsi+16]",
    "cmp rax, 4096",
    "jb .L__firn_segv_null",
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
    ".L__firn_segv_null:",
    "mov eax, 1",
    "mov edi, 2",
    "lea rsi, [rip+.L__firn_null_msg]",
    "mov edx, 31",
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
    ".L__firn_null_msg:",
    ".byte 102,105,114,110,58,32,110,117,108,108,32,112,111,105,110,116,101,114,32,100,101,114,101,102,101,114,101,110,99,101,10",
];

static SPAWNS_THREADS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn spawns_threads() -> bool {
    SPAWNS_THREADS.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn prepare(m: &crate::fir::Module) {
    let spawns = m.funcs.iter().any(|f| {
        f.blocks
            .iter()
            .any(|b| b.insts.iter().any(|i| matches!(i.op, crate::fir::Op::ThreadSpawn { .. })))
    });
    SPAWNS_THREADS.store(spawns, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn enabled() -> bool {
    (crate::checkmode::is_checked() || spawns_threads())
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

pub(crate) fn emit_thread_alt_stack(e: &mut crate::codegen_x86::Emitter) {
    for l in THREAD_ALT {
        e.line(l);
    }
}

pub(crate) const START_A64: &[&str] = &[
    "sub sp, sp, #64",
    "mov x0, #0",
    "mov x1, #65536",
    "mov x2, #3",
    "mov x3, #34",
    "movn x4, #0",
    "mov x5, #0",
    "mov x8, #222",
    "svc #0",
    "str x0, [sp]",
    "str xzr, [sp, #8]",
    "mov x1, #65536",
    "str x1, [sp, #16]",
    "mov x0, sp",
    "mov x1, xzr",
    "mov x8, #132",
    "svc #0",
    "adr x0, .L__firn_segv",
    "str x0, [sp, #32]",
    "movz x0, #4",
    "movk x0, #0x8800, lsl #16",
    "str x0, [sp, #40]",
    "str xzr, [sp, #48]",
    "str xzr, [sp, #56]",
    "mov x0, #11",
    "add x1, sp, #32",
    "mov x2, xzr",
    "mov x3, #8",
    "mov x8, #134",
    "svc #0",
    "add sp, sp, #64",
];

pub(crate) const THREAD_ALT_A64: &[&str] = &[
    "add x0, x1, #16",
    "sub sp, sp, #32",
    "str x0, [sp]",
    "str xzr, [sp, #8]",
    "mov x1, #32768",
    "str x1, [sp, #16]",
    "mov x0, sp",
    "mov x1, xzr",
    "mov x8, #132",
    "svc #0",
    "add sp, sp, #32",
];

pub(crate) const TAIL_A64: &[&str] = &[
    ".L__firn_segv:",
    "ldr x3, [x1, #16]",
    "ldr x4, [x2, #432]",
    "sub x3, x3, x4",
    "mov x5, #1048576",
    "add x3, x3, x5",
    "mov x5, #2097152",
    "cmp x3, x5",
    "b.hi .L__firn_segv_other",
    "mov x0, #2",
    "adr x1, .L__firn_so_msg",
    "mov x2, #21",
    "mov x8, #64",
    "svc #0",
    "mov x0, #101",
    "mov x8, #94",
    "svc #0",
    ".L__firn_segv_other:",
    "ret",
    ".L__firn_so_msg:",
    ".byte 102,105,114,110,58,32,115,116,97,99,107,32,111,118,101,114,102,108,111,119,10",
    ".balign 4",
];

pub(crate) fn enabled_a64() -> bool {
    (crate::checkmode::is_checked() || spawns_threads())
        && !crate::target::shared()
        && !crate::prof::is_kernel()
}

pub(crate) fn emit_start_a64(e: &mut crate::codegen_x86::Emitter) {
    for l in START_A64 {
        e.line(l);
    }
}

pub(crate) fn emit_thread_alt_stack_a64(e: &mut crate::codegen_x86::Emitter) {
    for l in THREAD_ALT_A64 {
        e.line(l);
    }
}

pub(crate) fn emit_tail_a64(e: &mut crate::codegen_x86::Emitter) {
    for l in TAIL_A64 {
        if l.ends_with(':') {
            e.raw(l);
        } else {
            e.line(l);
        }
    }
}

const WINDOWS_STACK_GUARANTEE: u32 = 0x8000;
const WINDOWS_STACK_OVERFLOW_CODE: i32 = -1073741571;
const WINDOWS_HANDLER: &str = "__firn_win_guard";

pub(crate) fn enabled_windows() -> bool {
    (crate::checkmode::is_checked() || spawns_threads())
        && crate::target::windows()
        && !crate::target::shared()
        && !crate::prof::is_kernel()
}

pub(crate) fn windows_start_asm(add_handler: &str, set_guarantee: &str) -> String {
    format!(
        "    mov edi, 1\n    lea rsi, [rip + {handler}]\n    call {add}\n    sub rsp, 16\n    mov dword ptr [rsp], {size}\n    mov rdi, rsp\n    call {set}\n    add rsp, 16\n",
        handler = WINDOWS_HANDLER,
        add = add_handler,
        set = set_guarantee,
        size = WINDOWS_STACK_GUARANTEE,
    )
}

pub(crate) fn windows_thread_asm(set_guarantee: &str) -> String {
    format!(
        "    mov dword ptr [rsp+160], {size}\n    lea rdi, [rsp+160]\n    call {set}\n",
        set = set_guarantee,
        size = WINDOWS_STACK_GUARANTEE,
    )
}

pub(crate) fn windows_handler_asm(get_std_handle: &str, write_file: &str, terminate_process: &str) -> String {
    format!(
        "{handler}:\n    mov rax, qword ptr [rcx]\n    cmp dword ptr [rax], {code}\n    jne .L__firn_win_other\n    sub rsp, 56\n    mov ecx, -12\n    call [rip + {get}]\n    mov rcx, rax\n    lea rdx, [rip + .L__firn_win_msg]\n    mov r8d, 21\n    lea r9, [rsp + 48]\n    mov qword ptr [rsp + 32], 0\n    call [rip + {write}]\n    mov rcx, -1\n    mov edx, 101\n    call [rip + {exit}]\n.L__firn_win_other:\n    xor eax, eax\n    ret\n.L__firn_win_msg:\n    .byte 102,105,114,110,58,32,115,116,97,99,107,32,111,118,101,114,102,108,111,119,10\n",
        handler = WINDOWS_HANDLER,
        code = WINDOWS_STACK_OVERFLOW_CODE,
        get = get_std_handle,
        write = write_file,
        exit = terminate_process,
    )
}
