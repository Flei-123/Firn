// SPDX-License-Identifier: GPL-2.0-only
//! **Round 80 — the target machine.** **Round ARM-FREESTANDING — and the
//! machine underneath it.**
//!
//! Up to round 79 there was exactly ONE machine: x86-64. It was not written
//! anywhere, because it did not have to be — every place that produced
//! machine code produced that one. That is precisely what made Firn a
//! one-machine language: the boundary between the intermediate
//! representation and the machine was not drawn, it was only *believed*.
//!
//! This file draws it. It holds
//!
//!   * which machine the current compilation is for (`--target=`),
//!   * **whether there is an operating system under it at all**,
//!   * the two names of the assembler and the linker that belong to it,
//!   * the small handful of assembler directives whose MEANING differs
//!     between the two ports of GNU as (`.align` counts bytes on x86 and
//!     powers of two on AArch64 — the same line would mean something else).
//!
//! Everything else about a machine lives in its code generator
//! (`codegen_x86.rs`, `codegen_a64.rs`) and in `syscalls.rs`.
//!
//! ## Two axes, not one
//!
//! Round 80 had one question ("which instruction set?") and answered it with
//! one enum. A kernel asks a second, entirely independent one: **is there an
//! operating system underneath?** The two do not fold into each other —
//! x86-64 with Linux under it and x86-64 with nothing under it share every
//! instruction and share no system call at all.
//!
//! So a target is a pair `(Arch, Os)`, and the four combinations get the
//! four names
//!
//! ```text
//!   x86_64-linux    aarch64-linux     an operating system is there
//!   x86_64-none     aarch64-none      nothing is there
//! ```
//!
//! **Why `-none`.** The name is not invented here. In the triple convention
//! that GNU binutils and LLVM both use (`<arch>-<vendor>-<os>`), the literal
//! string `none` in the operating system field is what a bare-metal target
//! is called — `aarch64-none-elf` is the name of the cross toolchain that
//! builds kernels for this very machine, and `x86_64-unknown-none` is what
//! the same idea is called on the other one. Taking that word means a reader
//! who has ever built firmware knows what the target is before reading a
//! line of this file; inventing `firn-bare` or `-kernel` would have meant
//! the opposite. Both long forms are accepted as aliases (§`flag_set`).
//!
//! ## What `-none` *means*, concretely
//!
//! 1. **No system calls.** `syscall` is refused at the line where it stands
//!    (`prof.rs`), because underneath there is nobody to accept one. Not a
//!    warning, not a quiet zero — an error.
//! 2. **No start out of a C runtime.** No `_start`, no `crt*.o`, no dynamic
//!    loader. The entry point belongs to whoever links the image.
//! 3. **A relocatable ELF object file** (`ET_REL`) as the output, to be tied
//!    together with a linker script of one's own.
//! 4. **A panic goes to `osum_panic`** — an external symbol the kernel
//!    defines itself (SPEC §2, `panic_rt.rs`, `panic_rt_a64.rs`).
//!
//! Points 1–4 are exactly what `profile kernel` has meant in the SOURCE
//! since round 52. A freestanding target is therefore not a second, parallel
//! mechanism: **it is the command line half of the same switch**, and it
//! turns the profile on (`prof::define`). Saying `profile app` and
//! `--target=x86_64-none` in one compilation is a contradiction and is
//! reported as one.
//!
//! That is the whole reason the freestanding x86 target can be checked so
//! sharply: `firnc --target=x86_64-none` and `firnc` on a source that says
//! `profile kernel` have to produce the SAME OCTETS, and
//! `tools/freestanding/none.sh` compares them.
//!
//! **The x86-64 path must not change.** Every function here answers for
//! `Target::X86_64` exactly what stood in the source before, character for
//! character; everything else is the addition.

use std::cell::Cell;

/// The instruction set. This is round 80's question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

/// What lies underneath. This is this round's question, and it is
/// independent of the one above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    /// Linux: system calls, an ELF executable, a `_start` of our own.
    Linux,
    /// Nothing at all: freestanding, an ELF object file, no system calls.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// `x86_64-linux` — the default, and the one path that must not change.
    X86_64,
    /// `aarch64-linux` — round 80.
    Aarch64,
    /// `x86_64-none` — freestanding x86-64.
    X86_64None,
    /// `aarch64-none` — freestanding AArch64. The one this round exists for.
    Aarch64None,
}

impl Target {
    /// The name as it is written on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Target::X86_64 => "x86_64-linux",
            Target::Aarch64 => "aarch64-linux",
            Target::X86_64None => "x86_64-none",
            Target::Aarch64None => "aarch64-none",
        }
    }
    /// The instruction set — everything that is about the MACHINE and not
    /// about what runs under it asks this and nothing else.
    pub fn arch(self) -> Arch {
        match self {
            Target::X86_64 | Target::X86_64None => Arch::X86_64,
            Target::Aarch64 | Target::Aarch64None => Arch::Aarch64,
        }
    }
    /// What lies underneath.
    pub fn os(self) -> Os {
        match self {
            Target::X86_64 | Target::Aarch64 => Os::Linux,
            Target::X86_64None | Target::Aarch64None => Os::None,
        }
    }
    /// Is there no operating system under this target?
    pub fn is_freestanding(self) -> bool {
        self.os() == Os::None
    }
    /// The assembler for this machine. Depends on the ARCH alone: the
    /// binutils that assemble A64 do not care whether Linux is going to run
    /// the result.
    pub fn assembler(self) -> &'static str {
        match self.arch() {
            Arch::X86_64 => "as",
            Arch::Aarch64 => "aarch64-linux-gnu-as",
        }
    }
    /// The arguments the assembler needs in front of `-o`.
    pub fn as_flags(self) -> &'static [&'static str] {
        match self.arch() {
            Arch::X86_64 => &["--64"],
            Arch::Aarch64 => &[],
        }
    }
    /// The linker for this machine. A freestanding target does not link at
    /// all (`main.rs` stops at the object file), so this answer is the same
    /// for both members of an arch — and it is the honest one for the day
    /// somebody links an object of ours by hand.
    pub fn linker(self) -> &'static str {
        match self.arch() {
            Arch::X86_64 => "ld",
            Arch::Aarch64 => "aarch64-linux-gnu-ld",
        }
    }
    /// `.align N` counts BYTES in the x86 port of GNU as and POWERS OF TWO
    /// in the AArch64 port. `.balign` counts bytes in both — but writing it
    /// on x86 too would change the emitted text of the existing path, and
    /// round 80 promised not to. So the directive is asked for, not written.
    pub fn align_directive(self, bytes: u64) -> String {
        match self.arch() {
            Arch::X86_64 => format!(".align {}", bytes),
            Arch::Aarch64 => format!(".balign {}", bytes),
        }
    }
}

thread_local! {
    static ACTIVE: Cell<Target> = const { Cell::new(Target::X86_64) };
}

/// `--target=<name>`. `Err` = unknown name.
///
/// The long triple forms are accepted because that is what the cross
/// toolchains are called, and somebody who has just typed
/// `aarch64-none-elf-gcc` should not have to guess our shorter spelling.
pub fn flag_set(name: &str) -> Result<(), String> {
    let t = match name {
        "x86_64-linux" | "x86-64-linux" | "x86_64" => Target::X86_64,
        "aarch64-linux" | "arm64-linux" | "aarch64" => Target::Aarch64,
        "x86_64-none" | "x86-64-none" | "x86_64-unknown-none" | "x86_64-none-elf" => {
            Target::X86_64None
        }
        "aarch64-none" | "arm64-none" | "aarch64-unknown-none" | "aarch64-none-elf" => {
            Target::Aarch64None
        }
        other => {
            return Err(format!(
                "unknown target '{}' (allowed: x86_64-linux, aarch64-linux, \
                 x86_64-none, aarch64-none)",
                other
            ))
        }
    };
    ACTIVE.with(|a| a.set(t));
    Ok(())
}

/// The machine of this compilation unit.
pub fn active() -> Target {
    ACTIVE.with(|a| a.get())
}

/// The instruction set of this compilation unit.
pub fn arch() -> Arch {
    active().arch()
}

/// Is there no operating system underneath?
pub fn freestanding() -> bool {
    active().is_freestanding()
}

/// `.align`/`.balign` of the active machine (see `Target::align_directive`).
pub fn align(bytes: u64) -> String {
    active().align_directive(bytes)
}

/// Reset — only for the module tests, which compile several programs in one
/// process (the same reason `prof::reset` exists).
#[cfg(test)]
pub fn reset() {
    ACTIVE.with(|a| a.set(Target::X86_64));
}

/// Set the target directly. Only for the module tests; the command line goes
/// through `flag_set`.
#[cfg(test)]
pub fn set(t: Target) {
    ACTIVE.with(|a| a.set(t));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_x86_and_align_stays_byte_counted() {
        reset();
        assert_eq!(active(), Target::X86_64);
        assert_eq!(align(8), ".align 8");
        flag_set("aarch64-linux").unwrap();
        assert_eq!(active(), Target::Aarch64);
        assert_eq!(align(8), ".balign 8");
        reset();
    }

    #[test]
    fn unknown_target_is_refused() {
        assert!(flag_set("sparc").is_err());
    }

    #[test]
    fn the_two_axes_are_really_independent() {
        reset();
        for (name, arch, os) in [
            ("x86_64-linux", Arch::X86_64, Os::Linux),
            ("x86_64-none", Arch::X86_64, Os::None),
            ("aarch64-linux", Arch::Aarch64, Os::Linux),
            ("aarch64-none", Arch::Aarch64, Os::None),
        ] {
            flag_set(name).unwrap();
            assert_eq!(active().arch(), arch, "{}", name);
            assert_eq!(active().os(), os, "{}", name);
            assert_eq!(active().name(), name, "{}", name);
        }
        reset();
    }

    #[test]
    fn a_freestanding_target_uses_the_same_binutils_as_its_arch() {
        // The assembler is a property of the INSTRUCTION SET. A round that
        // got this wrong would look for a toolchain nobody has installed.
        reset();
        flag_set("x86_64-none").unwrap();
        assert_eq!(active().assembler(), "as");
        assert_eq!(active().as_flags(), &["--64"]);
        assert_eq!(align(8), ".align 8");
        flag_set("aarch64-none").unwrap();
        assert_eq!(active().assembler(), "aarch64-linux-gnu-as");
        assert!(active().as_flags().is_empty());
        assert_eq!(align(8), ".balign 8");
        reset();
    }

    #[test]
    fn the_long_triple_names_mean_the_same_thing() {
        reset();
        for n in ["aarch64-none", "aarch64-none-elf", "aarch64-unknown-none", "arm64-none"] {
            flag_set(n).unwrap();
            assert_eq!(active(), Target::Aarch64None, "{}", n);
        }
        reset();
    }

    #[test]
    fn only_the_none_targets_are_freestanding() {
        reset();
        assert!(!freestanding());
        flag_set("aarch64-linux").unwrap();
        assert!(!freestanding());
        flag_set("x86_64-none").unwrap();
        assert!(freestanding());
        flag_set("aarch64-none").unwrap();
        assert!(freestanding());
        reset();
    }
}
