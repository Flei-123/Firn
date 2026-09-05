// SPDX-License-Identifier: GPL-2.0-only
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
}
