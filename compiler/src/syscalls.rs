// SPDX-License-Identifier: MPL-2.0
//! **Round 80 — system call numbers per machine.**
//!
//! A `syscall` in Firn source carries a NUMBER, not a name:
//!
//! ```firn
//! syscall(1, 1, &buf[0], 3)      // write(1, buf, 3)
//! ```
//!
//! and that number is the **x86-64 Linux** number. It is written that way in
//! every test of this repository and in every module of the standard library
//! (`lib/std/rt.fi`: `const SYS_WRITE: i64 = 1`). Linux numbers its calls
//! differently on every architecture, though — on AArch64 `write` is 64, and
//! 1 is `io_destroy`. Compiling the same source for the second machine
//! without doing anything about it would not fail: it would run the WRONG
//! call.
//!
//! So the number in FIR is read here as the **canonical name** of the call —
//! "the call that is 1 on x86-64" — and this file is the one place that
//! translates that name into the number of the machine actually being
//! compiled for. FIR itself stays free of it; `codegen_x86.rs` never asks
//! (there the canonical number IS the number), `codegen_a64.rs` asks for
//! every single `svc`.
//!
//! Three answers are possible, and the third one is the honest part:
//!
//!   * `Direct(n)` — the same call, another number.
//!   * `AtFdcwd(n)` — the same call, another number AND another shape. The
//!     generic system call table that AArch64 uses has no `open`, only
//!     `openat`; the path-relative form takes a directory descriptor in
//!     front of the path, and `AT_FDCWD` (-100) is what makes it mean the
//!     same as `open`.
//!   * `Missing(reason)` — this call does not exist on AArch64 in any
//!     shape that a number and an argument shift could bridge (`fork`,
//!     `arch_prctl`). The compilation stops with that reason; it does not
//!     quietly emit something else.

/// What becomes of an x86-64 system call number on AArch64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum A64 {
    /// same arguments, this number
    Direct(u32),
    /// `AT_FDCWD` in front of the arguments, then this number
    AtFdcwd(u32),
    /// `fork()` -> `clone(SIGCHLD, 0, 0, 0, 0)`. The generic table has no
    /// `fork`; the call it is a special case of is there, and `SIGCHLD` as
    /// the flag word is exactly what makes it one.
    ForkClone(u32),
    /// `dup2(old, new)` -> `dup3(old, new, 0)`. Same difference: only the
    /// flag carrying form survived into the generic table.
    Dup3(u32),
    /// `arch_prctl(ARCH_SET_FS, p)` -> `msr tpidr_el0, p`. This one is not a
    /// system call at all here: AArch64 lets EL0 write its own thread
    /// pointer, so what costs a system call on x86 costs one instruction.
    SetThreadPointer,
    /// no equivalent — with the reason for the error message
    Missing(&'static str),
}

/// `ARCH_SET_FS`, the only `arch_prctl` request that is translated.
pub const ARCH_SET_FS: i64 = 0x1002;
/// `SIGCHLD` — the flag word that turns `clone` into `fork`.
pub const SIGCHLD: i64 = 17;

/// `AT_FDCWD` — "relative to the working directory", the value that turns
/// `openat` back into `open`.
pub const AT_FDCWD: i64 = -100;

/// The table. Left the canonical (x86-64) number, right what AArch64 makes
/// of it. Sorted by the left column; the name in the comment is the one
/// both sides carry in `unistd.h`.
const TABLE: &[(i64, A64)] = &[
    (0, A64::Direct(63)),            // read
    (1, A64::Direct(64)),            // write
    (2, A64::AtFdcwd(56)),           // open      -> openat
    (3, A64::Direct(57)),            // close
    (5, A64::Direct(80)),            // fstat
    (8, A64::Direct(62)),            // lseek
    (9, A64::Direct(222)),           // mmap
    (10, A64::Direct(226)),          // mprotect
    (11, A64::Direct(215)),          // munmap
    (12, A64::Direct(214)),          // brk
    (13, A64::Direct(134)),          // rt_sigaction
    (14, A64::Direct(135)),          // rt_sigprocmask
    (16, A64::Direct(29)),           // ioctl
    (17, A64::Direct(67)),           // pread64
    (18, A64::Direct(68)),           // pwrite64
    (19, A64::Direct(65)),           // readv
    (20, A64::Direct(66)),           // writev
    (24, A64::Direct(124)),          // sched_yield
    (28, A64::Direct(233)),          // madvise
    (32, A64::Direct(23)),           // dup
    (33, A64::Dup3(24)),             // dup2      -> dup3
    (35, A64::Direct(101)),          // nanosleep
    (39, A64::Direct(172)),          // getpid
    (41, A64::Direct(198)),          // socket
    (42, A64::Direct(203)),          // connect
    (43, A64::Direct(202)),          // accept
    (44, A64::Direct(206)),          // sendto
    (45, A64::Direct(207)),          // recvfrom
    (46, A64::Direct(211)),          // sendmsg
    (47, A64::Direct(212)),          // recvmsg
    (48, A64::Direct(210)),          // shutdown
    (49, A64::Direct(200)),          // bind
    (50, A64::Direct(201)),          // listen
    (51, A64::Direct(204)),          // getsockname
    (52, A64::Direct(205)),          // getpeername
    (53, A64::Direct(199)),          // socketpair
    (54, A64::Direct(208)),          // setsockopt
    (55, A64::Direct(209)),          // getsockopt
    // clone(2) is 220 on AArch64, but the argument ORDER differs (tls and
    // child_tid change places) and the child comes back without a usable
    // thread pointer. `Op::ThreadSpawn` is the instruction that would have
    // to know that, not a number table -- and it does not, yet.
    (56, A64::Missing("clone(2): the argument order differs on aarch64 (see Op::ThreadSpawn)")),
    (57, A64::ForkClone(220)),       // fork      -> clone(SIGCHLD)
    (59, A64::Direct(221)),          // execve
    (60, A64::Direct(93)),           // exit
    (61, A64::Direct(260)),          // wait4
    (62, A64::Direct(129)),          // kill
    (63, A64::Direct(160)),          // uname
    (79, A64::Direct(17)),           // getcwd
    // Round ABSCHLUSS (Certus): the same shape as `open` two lines up --
    // the generic table has no `mkdir`, only `mkdirat`, and AT_FDCWD in
    // front of the path makes it mean the same. Without this line every
    // program that links lib/pdf/down.fi (the download folder) was
    // untranslatable for the phone, and that is the whole browser.
    (83, A64::AtFdcwd(34)),          // mkdir     -> mkdirat
    (96, A64::Direct(169)),          // gettimeofday
    (102, A64::Direct(174)),         // getuid
    (107, A64::Direct(175)),         // geteuid
    (158, A64::SetThreadPointer),    // arch_prctl(ARCH_SET_FS) -> msr tpidr_el0
    (186, A64::Direct(178)),         // gettid
    (200, A64::Direct(131)),         // tgkill
    (202, A64::Direct(98)),          // futex
    (217, A64::Direct(61)),          // getdents64
    (228, A64::Direct(113)),         // clock_gettime
    (231, A64::Direct(94)),          // exit_group
    (257, A64::Direct(56)),          // openat
    (262, A64::Direct(79)),          // newfstatat
    (288, A64::Direct(242)),         // accept4
    (318, A64::Direct(278)),         // getrandom
];

/// The AArch64 form of the canonical (x86-64) system call number `n`.
pub fn aarch64(n: i64) -> Option<A64> {
    TABLE.iter().find(|(k, _)| *k == n).map(|(_, v)| *v)
}

// ====================================================================
// ROUND WASM -- the same numbers, read by a browser.
// ====================================================================
//
// In the browser there is no kernel. There is a HOST: the JavaScript that
// instantiated the module. What a Linux system call becomes there is
// decided here, in one table, by the same canonical x86-64 number -- and
// the third answer of the aarch64 table has a sharper edge here: a call
// the browser does not have is refused at COMPILE time, with its name and
// the reason (`codegen_wasm.rs` adds the path through which `main`
// reaches it). Nothing is emulated that would behave differently from the
// kernel's answer; where a call has an exact equivalent under "one
// process, one thread, no files" it gets that equivalent.

/// What becomes of an x86-64 system call number on `wasm32-browser`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wasm {
    /// `firn.write(fd, buf, len)` -- the host decides what fd 1 and 2 are
    /// (the console, the terminal)
    Write,
    /// `firn.read(fd, buf, len)` -- standard input, if the host has one
    Read,
    /// `firn.exit(code)` -- `exit` and `exit_group` alike: one thread
    Exit,
    /// the host's nanosecond clock, split into a `timespec`
    ClockGettime,
    /// `firn.random(buf, len)` -- `crypto.getRandomValues` in a browser
    Getrandom,
    /// `firn.sleep_ns(ns)`
    Nanosleep,
    /// anonymous memory on top of `memory.grow` (`wasm_rt.rs`)
    Mmap,
    /// back into the free list of `wasm_rt.rs`
    Munmap,
    /// the futex of a process with exactly one thread (`wasm_rt.rs`)
    Futex,
    /// a call whose answer is a fixed number here
    Constant(i64),
    /// no counterpart -- the reason goes into the error message
    Missing(&'static str),
}

const NO_FILES: &str = "a browser page has no files and no file descriptors";
const NO_SOCKETS: &str = "a browser page has no sockets (fetch/WebSocket are not system calls)";
const NO_PROCESSES: &str = "a browser page cannot start, wait for or signal processes";
const NO_THREADS: &str = "threads are not supported on wasm32 yet";
const NO_SIGNALS: &str = "a browser page has no signals";

/// The table: the canonical number, the name both `unistd.h` spell, the
/// answer. Sorted by the number.
const WASM_TABLE: &[(i64, &str, Wasm)] = &[
    (0, "read", Wasm::Read),
    (1, "write", Wasm::Write),
    (2, "open", Wasm::Missing(NO_FILES)),
    (3, "close", Wasm::Missing(NO_FILES)),
    (4, "stat", Wasm::Missing(NO_FILES)),
    (5, "fstat", Wasm::Missing(NO_FILES)),
    (6, "lstat", Wasm::Missing(NO_FILES)),
    (7, "poll", Wasm::Missing(NO_FILES)),
    (8, "lseek", Wasm::Missing(NO_FILES)),
    (9, "mmap", Wasm::Mmap),
    (10, "mprotect", Wasm::Missing("WebAssembly memory has no page protection")),
    (11, "munmap", Wasm::Munmap),
    (12, "brk", Wasm::Missing("the program break does not exist in WebAssembly memory (use mmap)")),
    (13, "rt_sigaction", Wasm::Missing(NO_SIGNALS)),
    (14, "rt_sigprocmask", Wasm::Missing(NO_SIGNALS)),
    (16, "ioctl", Wasm::Missing(NO_FILES)),
    (17, "pread64", Wasm::Missing(NO_FILES)),
    (18, "pwrite64", Wasm::Missing(NO_FILES)),
    (19, "readv", Wasm::Missing(NO_FILES)),
    (20, "writev", Wasm::Missing(NO_FILES)),
    // One thread: yielding to nobody returns at once, as the kernel does
    // when no other thread is runnable.
    (24, "sched_yield", Wasm::Constant(0)),
    // Advice may be ignored -- the kernel is allowed to do exactly that.
    (28, "madvise", Wasm::Constant(0)),
    (32, "dup", Wasm::Missing(NO_FILES)),
    (33, "dup2", Wasm::Missing(NO_FILES)),
    (35, "nanosleep", Wasm::Nanosleep),
    // The one process of the page. 1 is as good a number as any, and it
    // is never 0 (which a caller could read as "the child").
    (39, "getpid", Wasm::Constant(1)),
    (41, "socket", Wasm::Missing(NO_SOCKETS)),
    (42, "connect", Wasm::Missing(NO_SOCKETS)),
    (43, "accept", Wasm::Missing(NO_SOCKETS)),
    (44, "sendto", Wasm::Missing(NO_SOCKETS)),
    (45, "recvfrom", Wasm::Missing(NO_SOCKETS)),
    (46, "sendmsg", Wasm::Missing(NO_SOCKETS)),
    (47, "recvmsg", Wasm::Missing(NO_SOCKETS)),
    (48, "shutdown", Wasm::Missing(NO_SOCKETS)),
    (49, "bind", Wasm::Missing(NO_SOCKETS)),
    (50, "listen", Wasm::Missing(NO_SOCKETS)),
    (51, "getsockname", Wasm::Missing(NO_SOCKETS)),
    (52, "getpeername", Wasm::Missing(NO_SOCKETS)),
    (53, "socketpair", Wasm::Missing(NO_SOCKETS)),
    (54, "setsockopt", Wasm::Missing(NO_SOCKETS)),
    (55, "getsockopt", Wasm::Missing(NO_SOCKETS)),
    (56, "clone", Wasm::Missing(NO_THREADS)),
    (57, "fork", Wasm::Missing(NO_PROCESSES)),
    (59, "execve", Wasm::Missing(NO_PROCESSES)),
    (60, "exit", Wasm::Exit),
    (61, "wait4", Wasm::Missing(NO_PROCESSES)),
    (62, "kill", Wasm::Missing(NO_PROCESSES)),
    (63, "uname", Wasm::Missing("a browser page has no kernel to name")),
    (72, "fcntl", Wasm::Missing(NO_FILES)),
    (79, "getcwd", Wasm::Missing(NO_FILES)),
    (96, "gettimeofday", Wasm::Missing("use clock_gettime (228), which the host provides")),
    (102, "getuid", Wasm::Missing("a browser page has no users")),
    (107, "geteuid", Wasm::Missing("a browser page has no users")),
    (158, "arch_prctl", Wasm::Missing(NO_THREADS)),
    // The one thread of the process.
    (186, "gettid", Wasm::Constant(1)),
    (200, "tgkill", Wasm::Missing(NO_SIGNALS)),
    (202, "futex", Wasm::Futex),
    (217, "getdents64", Wasm::Missing(NO_FILES)),
    (228, "clock_gettime", Wasm::ClockGettime),
    (231, "exit_group", Wasm::Exit),
    (257, "openat", Wasm::Missing(NO_FILES)),
    (262, "newfstatat", Wasm::Missing(NO_FILES)),
    (288, "accept4", Wasm::Missing(NO_SOCKETS)),
    (318, "getrandom", Wasm::Getrandom),
];

/// The browser form of the canonical (x86-64) system call number `n`,
/// with the call's name for the messages.
pub fn wasm(n: i64) -> Option<(&'static str, Wasm)> {
    WASM_TABLE.iter().find(|(k, _, _)| *k == n).map(|(_, nm, v)| (*nm, *v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_calls_the_library_makes_are_all_in_the_table() {
        // Exactly the numbers that appear in lib/std/*.fi and tests/*.fi.
        for n in [0i64, 1, 2, 3, 9, 11, 41, 42, 44, 45, 48, 49, 50, 51, 52, 54, 59, 60, 61, 231, 288]
        {
            assert!(aarch64(n).is_some(), "syscall {} missing from the table", n);
        }
    }

    #[test]
    fn write_is_not_the_same_number_on_both_machines() {
        assert_eq!(aarch64(1), Some(A64::Direct(64)));
        assert_eq!(aarch64(60), Some(A64::Direct(93)));
    }

    #[test]
    fn open_needs_the_directory_descriptor_in_front() {
        assert_eq!(aarch64(2), Some(A64::AtFdcwd(56)));
    }

    #[test]
    fn what_is_missing_says_so_rather_than_guessing() {
        // The one call that really has no shape here: `clone` itself, whose
        // arguments change places (that is `Op::ThreadSpawn`'s business).
        assert!(matches!(aarch64(56), Some(A64::Missing(_))));
        assert_eq!(aarch64(4711), None);
    }

    #[test]
    fn fork_and_dup2_survive_as_their_general_forms() {
        assert_eq!(aarch64(57), Some(A64::ForkClone(220)));
        assert_eq!(aarch64(33), Some(A64::Dup3(24)));
        assert_eq!(aarch64(158), Some(A64::SetThreadPointer));
    }

    #[test]
    fn the_table_is_sorted_and_free_of_duplicates() {
        for w in TABLE.windows(2) {
            assert!(w[0].0 < w[1].0, "table not sorted at {}", w[0].0);
        }
    }

    #[test]
    fn the_browser_table_is_sorted_and_names_every_call() {
        for w in WASM_TABLE.windows(2) {
            assert!(w[0].0 < w[1].0, "wasm table not sorted at {}", w[0].0);
        }
        assert_eq!(wasm(1), Some(("write", Wasm::Write)));
        assert!(matches!(wasm(2), Some(("open", Wasm::Missing(_)))));
        assert!(matches!(wasm(41), Some(("socket", Wasm::Missing(_)))));
        assert!(matches!(wasm(57), Some(("fork", Wasm::Missing(_)))));
        assert_eq!(wasm(9), Some(("mmap", Wasm::Mmap)));
        assert_eq!(wasm(4711), None);
    }

    #[test]
    fn every_call_of_the_aarch64_table_has_a_browser_answer() {
        // The two tables cover the same calls: a number the library uses
        // on one target is never silently unknown on the other.
        for (n, _) in TABLE {
            assert!(wasm(*n).is_some(), "syscall {} has no wasm32 answer", n);
        }
    }
}
