// SPDX-License-Identifier: GPL-2.0-only
//! **Round WINDOWS — PE/COFF, the Win64 calling convention and the imports.**
//!
//! Everything in this file exists because the second axis of a target
//! (`target.rs`, round ARM-FREESTANDING) got a third value: `Os::Windows`.
//! The instruction set is unchanged — every `mov`, every `imul`, every
//! `cmov` that `codegen_x86.rs` and `regalloc.rs` write for Linux is
//! written for Windows as well, character for character. What changes is
//! everything AROUND the instructions:
//!
//!   1. **The object format.** ELF cannot become a PE image. The COFF port
//!      of the same binutils (`x86_64-w64-mingw32-as` / `-ld`) does it, and
//!      it is used exactly the way `as`/`ld` are used on Linux: as an
//!      assembler and as a linker, never as a compiler.
//!   2. **The binding to the operating system.** There is no `syscall`
//!      instruction on Windows that a program may use; the numbers are not
//!      stable between Windows versions on purpose. Everything goes through
//!      `kernel32.dll` and its relatives, and it gets there through the
//!      **import table of the PE file** — which this file writes ITSELF
//!      (§ `idata_asm`) instead of pulling in an import library. No foreign
//!      object file enters the image.
//!   3. **The calling convention at the outer boundary.** Win64 is not
//!      System V (§ `thunk`).
//!   4. **The stack.** Windows grows a thread stack over guard pages; a
//!      frame bigger than one page has to walk down page by page or the
//!      guard is jumped over and the process dies (§ `chkstk_asm`).
//!
//! ## Why a thunk and not a second calling convention in the code generator
//!
//! Win64 puts the first four integer arguments in `rcx, rdx, r8, r9`,
//! demands **32 octets of shadow space** below the return address that the
//! callee may scribble on, and counts `rsi`, `rdi` and `xmm6`–`xmm15` as
//! callee-saved. System V puts six arguments in `rdi, rsi, rdx, rcx, r8,
//! r9`, has no shadow space, and counts `rsi`/`rdi` as scratch.
//!
//! Firn's **internal** calls do not have to be Win64. Nothing in Windows
//! looks at how one Firn function calls another; SPEC §13 already calls the
//! Firn-to-Firn convention a documented ABI of its own. What MUST be Win64
//! is every call that leaves the program — and every call that comes in
//! (a callback), which this round does not have.
//!
//! So the boundary gets ONE well-defined place: for every imported function
//! the compiler emits a **thunk** that takes its arguments the System V way,
//! rearranges them the Win64 way, allocates the shadow space and jumps
//! through the import address table. The register allocator, the frame
//! layout and both code generators stay untouched — which is exactly why
//! the Linux side cannot get worse.
//!
//! The price is honest and is written down in `docs/ROUND-WINDOWS.md`:
//! four to eight extra `mov`s per Win32 call, and a Firn function cannot be
//! handed to Windows as a callback yet (that would need the mirror image
//! thunk).

use std::cell::RefCell;
use std::collections::BTreeSet;

/// Prefix of every symbol this file invents. Same idea as `_F0.`
/// (`modules.rs`): reserved, and no source text can produce it.
pub const PREFIX: &str = "_Fwin.";

/// The stack probe. Called with the frame size in `rax`.
pub const CHKSTK: &str = "_Fwin.chkstk";

/// The stub that takes the LINUX system call register convention and hands
/// it to the seam. Everything that emits a `syscall` instruction as HAND
/// WRITTEN assembler (`panic_rt.rs`) writes a `call` to this instead.
pub const SYSSTUB: &str = "_Fwin.syscall";

/// From this frame size on a function has to probe the stack. One page.
pub const PAGE: u64 = 4096;

/// The Win32 functions this compiler knows: name, DLL, number of arguments.
///
/// The arity is not decoration — the thunk cannot be written without it
/// (arguments five and up travel over the stack, and where they land
/// depends on how many there are). An `extern fn` naming something that is
/// not in this table is refused with that reason rather than mis-called.
const KNOWN: &[(&str, &str, u32)] = &[
    // --- kernel32: process, memory, files, time -----------------------
    ("ExitProcess", "KERNEL32.dll", 1),
    ("GetStdHandle", "KERNEL32.dll", 1),
    ("WriteFile", "KERNEL32.dll", 5),
    ("ReadFile", "KERNEL32.dll", 5),
    ("CloseHandle", "KERNEL32.dll", 1),
    ("CreateFileW", "KERNEL32.dll", 7),
    ("DeleteFileW", "KERNEL32.dll", 1),
    ("GetFileAttributesW", "KERNEL32.dll", 1),
    ("SetFilePointerEx", "KERNEL32.dll", 4),
    ("FlushFileBuffers", "KERNEL32.dll", 1),
    ("VirtualAlloc", "KERNEL32.dll", 4),
    ("VirtualFree", "KERNEL32.dll", 3),
    ("VirtualProtect", "KERNEL32.dll", 4),
    ("GetLastError", "KERNEL32.dll", 0),
    ("QueryPerformanceCounter", "KERNEL32.dll", 1),
    ("QueryPerformanceFrequency", "KERNEL32.dll", 1),
    ("GetSystemTimeAsFileTime", "KERNEL32.dll", 1),
    ("Sleep", "KERNEL32.dll", 1),
    ("SwitchToThread", "KERNEL32.dll", 0),
    ("GetCommandLineW", "KERNEL32.dll", 0),
    ("GetCurrentDirectoryW", "KERNEL32.dll", 2),
    ("GetCurrentProcessId", "KERNEL32.dll", 0),
    ("GetCurrentThreadId", "KERNEL32.dll", 0),
    // The stack bounds of this thread. Windows 8 and later; the
    // collector needs them, because there is no /proc to read.
    ("GetCurrentThreadStackLimits", "KERNEL32.dll", 2),
    ("GetCurrentProcess", "KERNEL32.dll", 0),
    ("DuplicateHandle", "KERNEL32.dll", 7),
    ("GetEnvironmentStringsW", "KERNEL32.dll", 0),
    // --- ws2_32: the sockets ------------------------------------------
    ("WSAStartup", "WS2_32.dll", 2),
    ("WSAGetLastError", "WS2_32.dll", 0),
    ("socket", "WS2_32.dll", 3),
    ("connect", "WS2_32.dll", 3),
    ("send", "WS2_32.dll", 4),
    ("recv", "WS2_32.dll", 4),
    ("closesocket", "WS2_32.dll", 1),
    ("shutdown", "WS2_32.dll", 2),
    ("bind", "WS2_32.dll", 3),
    ("listen", "WS2_32.dll", 2),
    ("accept", "WS2_32.dll", 3),
    ("setsockopt", "WS2_32.dll", 5),
    ("getsockname", "WS2_32.dll", 3),
    ("ioctlsocket", "WS2_32.dll", 3),
    // --- ws2_32: what round CERTUS-WINDOWS had to add ------------------
    // `sendto`/`recvfrom` WITH an address. Round MERGE-WIN bound only the
    // connected form and wrote down that Certus does not need the other
    // one -- which was measured on the wrong tree: `lib/net/udp.fi` asks
    // every DNS question with `sendto(fd, .., addr, alen)`, and a UDP
    // socket that was never `connect`ed answers WSAENOTCONN to `send`.
    ("sendto", "WS2_32.dll", 6),
    ("recvfrom", "WS2_32.dll", 6),
    ("select", "WS2_32.dll", 5),
    // --- kernel32: directories, renaming, and looking a symbol up -----
    ("CreateDirectoryW", "KERNEL32.dll", 2),
    ("MoveFileExW", "KERNEL32.dll", 3),
    ("GetModuleHandleW", "KERNEL32.dll", 1),
    ("GetProcAddress", "KERNEL32.dll", 2),
    // --- user32: the window ------------------------------------------
    // Round MERGE-WIN 5.2 says "any GUI" is not bound and therefore not
    // reachable. This is where that stops being true. Everything here is
    // Win32 straight out of the DLL; no wrapper library, no widget set.
    ("RegisterClassW", "USER32.dll", 1),
    ("CreateWindowExW", "USER32.dll", 12),
    ("DestroyWindow", "USER32.dll", 1),
    ("ShowWindow", "USER32.dll", 2),
    ("UpdateWindow", "USER32.dll", 1),
    ("DefWindowProcW", "USER32.dll", 4),
    ("GetMessageW", "USER32.dll", 4),
    ("PeekMessageW", "USER32.dll", 5),
    ("TranslateMessage", "USER32.dll", 1),
    ("DispatchMessageW", "USER32.dll", 1),
    ("PostQuitMessage", "USER32.dll", 1),
    ("GetClientRect", "USER32.dll", 2),
    ("InvalidateRect", "USER32.dll", 3),
    ("ValidateRect", "USER32.dll", 2),
    ("GetDC", "USER32.dll", 1),
    ("ReleaseDC", "USER32.dll", 2),
    ("IsWindow", "USER32.dll", 1),
    ("SetWindowTextW", "USER32.dll", 2),
    ("LoadCursorW", "USER32.dll", 2),
    ("SetProcessDpiAwarenessContext", "USER32.dll", 1),
    ("SetProcessDPIAware", "USER32.dll", 0),
    ("GetDpiForWindow", "USER32.dll", 1),
    ("MsgWaitForMultipleObjectsEx", "USER32.dll", 5),
    ("AdjustWindowRectEx", "USER32.dll", 4),
    ("GetSystemMetrics", "USER32.dll", 1),
    ("GetKeyState", "USER32.dll", 1),
    ("SetCapture", "USER32.dll", 1),
    ("ReleaseCapture", "USER32.dll", 0),
    // --- gdi32: the DIB section and the one blit ----------------------
    ("CreateDIBSection", "GDI32.dll", 6),
    ("CreateCompatibleDC", "GDI32.dll", 1),
    ("SelectObject", "GDI32.dll", 2),
    ("BitBlt", "GDI32.dll", 9),
    ("DeleteObject", "GDI32.dll", 1),
    ("DeleteDC", "GDI32.dll", 1),
    ("SetTextColor", "GDI32.dll", 2),
    ("SetBkColor", "GDI32.dll", 2),
    ("SetBkMode", "GDI32.dll", 2),
    ("TextOutA", "GDI32.dll", 5),
    ("GetStockObject", "GDI32.dll", 1),
    // --- advapi32: the random source ----------------------------------
    // `SystemFunction036` IS `RtlGenRandom`; that is the name it is
    // exported under, and Microsoft's own header only gives it the other
    // one through a macro.
    ("SystemFunction036", "ADVAPI32.dll", 2),
];

/// DLL and arity of a known Win32 function.
pub fn known(name: &str) -> Option<(&'static str, u32)> {
    KNOWN.iter().find(|(n, _, _)| *n == name).map(|(_, d, a)| (*d, *a))
}

thread_local! {
    /// Which imports this compilation unit really needs.
    static USED: RefCell<BTreeSet<String>> = RefCell::new(BTreeSet::new());
    /// Did any function need a stack probe?
    static PROBED: RefCell<bool> = const { RefCell::new(false) };
    /// Did the hand written runtime need the system call stub?
    static SYSSTUB_USED: RefCell<bool> = const { RefCell::new(false) };
}

/// Only for the module tests, which compile several programs in one process.
pub fn reset() {
    USED.with(|u| u.borrow_mut().clear());
    PROBED.with(|p| *p.borrow_mut() = false);
    SYSSTUB_USED.with(|p| *p.borrow_mut() = false);
}

/// Registers `name` as an import and yields the symbol a System V caller
/// jumps to. `None` = not a Win32 function this compiler knows.
pub fn note(name: &str) -> Option<String> {
    known(name)?;
    USED.with(|u| u.borrow_mut().insert(name.to_string()));
    Some(format!("{}{}", PREFIX, name))
}

/// The thunk symbol of an import (without registering it).
pub fn thunk(name: &str) -> String {
    format!("{}{}", PREFIX, name)
}

/// The import address table slot of an import.
fn imp(name: &str) -> String {
    format!("__imp_{}", name)
}

/// Records that a stack probe was emitted.
pub fn note_probe() {
    PROBED.with(|p| *p.borrow_mut() = true);
}

/// Records that the hand written runtime needs the system call stub.
pub fn note_sysstub() {
    SYSSTUB_USED.with(|p| *p.borrow_mut() = true);
}

/// Is the system call stub needed?
pub fn sysstub_used() -> bool {
    SYSSTUB_USED.with(|p| *p.borrow())
}

/// **The stub for hand written runtime assembler.**
///
/// `panic_rt.rs` writes its two `write(2, …)` calls and its
/// `exit_group(101)` as instructions, not as Firn — it has to, because it
/// runs with a stack frame it built itself and with the panic arguments in
/// registers of its own choosing. On Windows those `syscall` instructions
/// would be a crash (there is no Linux kernel below), so the same lines
/// call THIS instead, and it forwards to the seam.
///
/// In: `rax` = the canonical (x86-64 Linux) call number, `rdi, rsi, rdx,
/// r10, r8, r9` = the arguments — exactly the register set the instruction
/// itself reads. Out: `rax`, exactly as the instruction leaves it.
fn sysstub_asm() -> String {
    let mut s = String::new();
    s.push_str(&format!("\n.globl {}\n{}:\n", SYSSTUB, SYSSTUB));
    s.push_str("    # the Linux syscall register set -> the seam\n");
    s.push_str("    push rbp\n    mov rbp, rsp\n");
    // THE ALIGNMENT, and it is not decoration. This stub is jumped to out
    // of hand written runtime code whose `rsp` is whatever its own pushes
    // left behind; a Win32 function entered one word off dies inside the
    // first aligned SSE move of some system DLL, far away from the cause.
    // `leave` puts the stack back exactly, so forcing it here is free.
    s.push_str("    and rsp, -16\n");
    s.push_str("    sub rsp, 16\n");
    s.push_str("    mov qword ptr [rsp], r9\n"); // a6
    s.push_str("    mov r9, r8\n");              // a5
    s.push_str("    mov r8, r10\n");             // a4
    s.push_str("    mov rcx, rdx\n");            // a3
    s.push_str("    mov rdx, rsi\n");            // a2
    s.push_str("    mov rsi, rdi\n");            // a1
    s.push_str("    mov rdi, rax\n");            // the number
    s.push_str(&format!(
        "    call {}\n",
        crate::codegen_x86::label(crate::win_seam::SYSCALL_FN)
    ));
    s.push_str("    leave\n    ret\n");
    s
}

/// Was a stack probe emitted anywhere?
pub fn probed() -> bool {
    PROBED.with(|p| *p.borrow())
}

fn align_up(x: u64, a: u64) -> u64 {
    (x + a - 1) / a * a
}

/// **One System V → Win64 thunk.**
///
/// On entry the System V arguments sit in `rdi, rsi, rdx, rcx, r8, r9` and,
/// from the seventh on, at `[rbp+16]`, `[rbp+24]`, … On exit to the Win32
/// function they have to sit in `rcx, rdx, r8, r9` and, from the fifth on,
/// at `[rsp+32]`, `[rsp+40]`, … — above 32 octets of shadow space that
/// belongs to the callee.
///
/// The ORDER of the moves is the whole trick: `r8` and `r9` are Win64
/// argument registers 3 and 4 AND at the same time System V argument
/// registers 5 and 6. Writing `mov r8, rdx` before argument five has left
/// `r8` loses it. So the stack arguments go down first, `r8`/`r9` among
/// them, and only then the four register arguments in descending order.
fn thunk_asm(name: &str, argc: u32) -> String {
    let mut s = String::new();
    let n = argc as u64;
    // 32 octets of shadow + one word for every argument from the fifth on.
    let space = align_up(32 + 8 * n.saturating_sub(4), 16);
    s.push_str(&format!("\n.globl {}\n{}:\n", thunk(name), thunk(name)));
    s.push_str(&format!(
        "    # System V -> Win64 for {}({} arguments)\n",
        name, argc
    ));
    s.push_str("    push rbp\n    mov rbp, rsp\n");
    s.push_str(&format!("    sub rsp, {}\n", space));
    // Arguments seven and up: out of the CALLER's frame into the Win64 area.
    let mut i = 6u64;
    while i < n {
        s.push_str(&format!(
            "    mov rax, qword ptr [rbp+{}]\n    mov qword ptr [rsp+{}], rax\n",
            16 + 8 * (i - 6),
            8 * i
        ));
        i += 1;
    }
    // Arguments five and six: FIRST, because r8/r9 are about to be overwritten.
    if n >= 5 {
        s.push_str("    mov qword ptr [rsp+32], r8\n");
    }
    if n >= 6 {
        s.push_str("    mov qword ptr [rsp+40], r9\n");
    }
    // The four register arguments, in the one order that destroys nothing.
    if n >= 4 {
        s.push_str("    mov r9, rcx\n");
    }
    if n >= 3 {
        s.push_str("    mov r8, rdx\n");
    }
    if n >= 2 {
        s.push_str("    mov rdx, rsi\n");
    }
    if n >= 1 {
        s.push_str("    mov rcx, rdi\n");
    }
    s.push_str(&format!("    call [rip + {}]\n", imp(name)));
    // `leave` is `mov rsp, rbp` + `pop rbp` — the shadow space disappears
    // with the frame, whatever the callee did to it.
    s.push_str("    leave\n    ret\n");
    s
}

/// **The stack probe.**
///
/// A Windows thread stack is reserved but not committed. At its lower end
/// sits a PAGE_GUARD page; touching it makes the kernel commit one more
/// page and move the guard down. A function that subtracts 40 KiB from
/// `rsp` and then writes at the bottom of its frame jumps OVER the guard,
/// hits reserved but uncommitted memory, and the process dies with
/// `STATUS_ACCESS_VIOLATION` — not at the write that was wrong, but at some
/// innocent looking local variable.
///
/// So every frame from one page on walks down page by page and touches each
/// one. `rax` holds the number of octets about to be subtracted; nothing
/// but `rax` and the flags is destroyed, because the prologue that calls
/// this still has the incoming arguments in the argument registers.
fn chkstk_asm() -> String {
    let mut s = String::new();
    s.push_str(&format!("\n.globl {}\n{}:\n", CHKSTK, CHKSTK));
    s.push_str("    # Windows guard page: touch every page of the coming frame.\n");
    s.push_str("    push rcx\n");
    s.push_str("    mov rcx, rsp\n");
    s.push_str(".Lfw_probe:\n");
    s.push_str(&format!("    cmp rax, {}\n", PAGE));
    s.push_str("    jbe .Lfw_last\n");
    s.push_str(&format!("    sub rcx, {}\n", PAGE));
    s.push_str("    mov byte ptr [rcx], 0\n");
    s.push_str(&format!("    sub rax, {}\n", PAGE));
    s.push_str("    jmp .Lfw_probe\n");
    s.push_str(".Lfw_last:\n");
    s.push_str("    sub rcx, rax\n");
    s.push_str("    mov byte ptr [rcx], 0\n");
    s.push_str("    pop rcx\n");
    s.push_str("    ret\n");
    s
}

/// **The import table, written by hand.**
///
/// A PE file binds to a DLL through five parallel tables, and every real
/// tool chain hides them inside an "import library" (`libkernel32.a`) whose
/// object files then end up in the image. Firn's rule that no foreign code
/// runs inside the program is easier to keep than to explain here: the five
/// tables are pure DATA, they are twenty lines of assembler per DLL, and
/// the linker's own PE script already collects `.idata$2` … `.idata$7` in
/// the right order and terminates the directory. So the compiler writes
/// them, and `-lkernel32` never appears on the command line.
///
///   * `.idata$2` — one 20 octet descriptor per DLL (lookup table, time
///     stamp, forwarder chain, name, address table).
///   * `.idata$4` — the import LOOKUP table: what the loader reads.
///   * `.idata$5` — the import ADDRESS table: what the loader overwrites
///     with the real addresses, and what our thunks call through.
///   * `.idata$6` — hint/name pairs, two octet aligned.
///   * `.idata$7` — the DLL names.
fn idata_asm(used: &[String]) -> String {
    let mut dlls: Vec<&'static str> = Vec::new();
    for u in used {
        if let Some((d, _)) = known(u) {
            if !dlls.contains(&d) {
                dlls.push(d);
            }
        }
    }
    let mut s = String::new();
    s.push_str("\n# ---- the import table of this program, written by the compiler ----\n");
    s.push_str(".section .idata$2\n");
    for (k, d) in dlls.iter().enumerate() {
        s.push_str(&format!("    .rva .Lfw_ilt{}\n", k));
        s.push_str("    .long 0\n    .long 0\n");
        s.push_str(&format!("    .rva .Lfw_dll{}\n", k));
        s.push_str(&format!("    .rva .Lfw_iat{}\n", k));
        let _ = d;
    }
    // Lookup tables.
    s.push_str(".section .idata$4\n");
    for (k, d) in dlls.iter().enumerate() {
        s.push_str(&format!(".Lfw_ilt{}:\n", k));
        for u in used {
            if known(u).map(|(dd, _)| dd) == Some(*d) {
                s.push_str(&format!("    .rva .Lfw_n_{}\n    .long 0\n", u));
            }
        }
        s.push_str("    .quad 0\n");
    }
    // Address tables — the slots the loader fills in.
    s.push_str(".section .idata$5\n");
    for (k, d) in dlls.iter().enumerate() {
        s.push_str(&format!(".Lfw_iat{}:\n", k));
        for u in used {
            if known(u).map(|(dd, _)| dd) == Some(*d) {
                s.push_str(&format!("{}:\n    .rva .Lfw_n_{}\n    .long 0\n", imp(u), u));
            }
        }
        s.push_str("    .quad 0\n");
    }
    // Hint/name pairs.
    s.push_str(".section .idata$6\n");
    for u in used {
        s.push_str(&format!(
            ".Lfw_n_{}:\n    .short 0\n    .asciz \"{}\"\n    .balign 2\n",
            u, u
        ));
    }
    // DLL names.
    s.push_str(".section .idata$7\n");
    for (k, d) in dlls.iter().enumerate() {
        s.push_str(&format!(".Lfw_dll{}:\n    .asciz \"{}\"\n", k, d));
    }
    s.push_str(".text\n");
    s
}

/// **The entry point.**
///
/// On Linux `_start` gets the initial stack pointer and hands it to `main`
/// as the first argument, so that a program can reach `argc`/`argv`
/// (`docs/SELF_HOSTING.md` §2). Windows hands over nothing at all: the
/// command line is a single UTF-16 string behind `GetCommandLineW`.
///
/// So the seam (`win_seam.rs`, written in Firn) builds the same block
/// Linux would have put on the stack — `[argc][argv0]…[0][envp…][0]` — and
/// gives back its address; `main` cannot tell the difference.
///
/// `and rsp, -16` first: the alignment `_start` is entered with is
/// documented, but a program that comes out one word off dies inside the
/// first `movaps` of some system DLL, and that is a very expensive hour.
pub fn start_asm(gc_init: Option<&str>, seam_init: &str, argv_sym: &str, main_sym: &str) -> String {
    let mut s = String::new();
    s.push_str(".globl _start\n_start:\n");
    s.push_str("    and rsp, -16\n");
    s.push_str("    sub rsp, 32\n");
    // The seam first: it opens the standard handles, and everything after
    // it — the collector included — may want to report an error.
    s.push_str(&format!("    call {}\n", seam_init));
    if let Some(g) = gc_init {
        s.push_str(&format!("    call {}\n", g));
    }
    s.push_str(&format!("    call {}\n", argv_sym));
    s.push_str("    mov rdi, rax\n");
    s.push_str(&format!("    call {}\n", main_sym));
    s.push_str("    mov ecx, eax\n");
    s.push_str(&format!("    call [rip + {}]\n", imp("ExitProcess")));
    s.push_str("    hlt\n");
    s
}

/// Everything the Windows target adds at the END of the assembly file:
/// the thunks, the stack probe and the import table.
pub fn runtime_asm() -> String {
    let used: Vec<String> = USED.with(|u| u.borrow().iter().cloned().collect());
    let mut s = String::new();
    if used.is_empty() && !probed() && !sysstub_used() {
        return s;
    }
    s.push_str("\n# ==== round WINDOWS: the boundary to Win32 ====\n");
    s.push_str(".text\n");
    for u in &used {
        let (_, argc) = known(u).expect("registered import is known");
        s.push_str(&thunk_asm(u, argc));
    }
    if probed() {
        s.push_str(&chkstk_asm());
    }
    if sysstub_used() {
        s.push_str(&sysstub_asm());
    }
    if !used.is_empty() {
        s.push_str(&idata_asm(&used));
    }
    s
}

/// Every import the seam needs, registered up front. `_start` calls
/// `ExitProcess` through the address table directly, and a program that
/// never mentions a Win32 function still has to be able to stop.
pub fn note_baseline() {
    for n in crate::win_seam::BASELINE {
        let _ = note(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_functions_become_imports() {
        reset();
        assert_eq!(note("WriteFile"), Some("_Fwin.WriteFile".to_string()));
        assert_eq!(note("PostQuantumDance"), None);
    }

    /// The order of the moves is the bug this thunk exists to avoid.
    #[test]
    fn five_and_six_go_down_before_r8_is_overwritten() {
        let a = thunk_asm("CreateFileW", 7);
        let five = a.find("mov qword ptr [rsp+32], r8").unwrap();
        let overwrite = a.find("mov r8, rdx").unwrap();
        assert!(five < overwrite, "argument five is lost:\n{}", a);
        let six = a.find("mov qword ptr [rsp+40], r9").unwrap();
        let ow9 = a.find("mov r9, rcx").unwrap();
        assert!(six < ow9, "argument six is lost:\n{}", a);
        // The seventh argument comes out of the caller's frame.
        assert!(a.contains("mov rax, qword ptr [rbp+16]"), "{}", a);
        assert!(a.contains("mov qword ptr [rsp+48], rax"), "{}", a);
    }

    #[test]
    fn the_shadow_space_is_there_and_the_stack_stays_aligned() {
        // 32 octets shadow, nothing else, rounded to 16.
        assert!(thunk_asm("CloseHandle", 1).contains("sub rsp, 32"));
        assert!(thunk_asm("VirtualAlloc", 4).contains("sub rsp, 32"));
        // 32 + 1 word = 40 -> 48.
        assert!(thunk_asm("WriteFile", 5).contains("sub rsp, 48"));
        // 32 + 3 words = 56 -> 64.
        assert!(thunk_asm("CreateFileW", 7).contains("sub rsp, 64"));
    }

    #[test]
    fn a_thunk_with_no_arguments_moves_nothing() {
        let a = thunk_asm("GetLastError", 0);
        assert!(!a.contains("mov rcx, rdi"), "{}", a);
        assert!(a.contains("call [rip + __imp_GetLastError]"), "{}", a);
    }

    #[test]
    fn the_import_table_has_one_descriptor_per_dll() {
        let used = vec!["WriteFile".to_string(), "socket".to_string()];
        let a = idata_asm(&used);
        assert_eq!(a.matches(".rva .Lfw_dll").count(), 2, "{}", a);
        assert!(a.contains(".asciz \"KERNEL32.dll\""), "{}", a);
        assert!(a.contains(".asciz \"WS2_32.dll\""), "{}", a);
        // Every table ends in its null entry.
        assert_eq!(a.matches("    .quad 0\n").count(), 4, "{}", a);
        assert!(a.contains("__imp_WriteFile:"), "{}", a);
    }

    #[test]
    fn the_probe_walks_page_by_page() {
        let a = chkstk_asm();
        assert!(a.contains("sub rcx, 4096"), "{}", a);
        assert!(a.contains("mov byte ptr [rcx], 0"), "{}", a);
        // and touches the remainder as well
        assert!(a.contains("sub rcx, rax"), "{}", a);
    }
}
