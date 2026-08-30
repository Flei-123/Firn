//! **Round 80 — the target machine.**
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
//!   * the two names of the assembler and the linker that belong to it,
//!   * the small handful of assembler directives whose MEANING differs
//!     between the two ports of GNU as (`.align` counts bytes on x86 and
//!     powers of two on AArch64 — the same line would mean something else).
//!
//! Everything else about a machine lives in its code generator
//! (`codegen_x86.rs`, `codegen_a64.rs`) and in `syscalls.rs`.
//!
//! **The x86-64 path must not change.** Every function here answers for
//! `Target::X86_64` exactly what stood in the source before, character for
//! character; the aarch64 answers are the additions.

use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    X86_64,
    Aarch64,
}

impl Target {
    /// The name as it is written on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Target::X86_64 => "x86_64-linux",
            Target::Aarch64 => "aarch64-linux",
        }
    }
    /// The assembler for this machine.
    pub fn assembler(self) -> &'static str {
        match self {
            Target::X86_64 => "as",
            Target::Aarch64 => "aarch64-linux-gnu-as",
        }
    }
    /// The arguments the assembler needs in front of `-o`.
    pub fn as_flags(self) -> &'static [&'static str] {
        match self {
            Target::X86_64 => &["--64"],
            Target::Aarch64 => &[],
        }
    }
    /// The linker for this machine.
    pub fn linker(self) -> &'static str {
        match self {
            Target::X86_64 => "ld",
            Target::Aarch64 => "aarch64-linux-gnu-ld",
        }
    }
    /// `.align N` counts BYTES in the x86 port of GNU as and POWERS OF TWO
    /// in the AArch64 port. `.balign` counts bytes in both — but writing it
    /// on x86 too would change the emitted text of the existing path, and
    /// round 80 promised not to. So the directive is asked for, not written.
    pub fn align_directive(self, bytes: u64) -> String {
        match self {
            Target::X86_64 => format!(".align {}", bytes),
            Target::Aarch64 => format!(".balign {}", bytes),
        }
    }
}

thread_local! {
    static ACTIVE: Cell<Target> = const { Cell::new(Target::X86_64) };
    /// ROUND MOBIL (Certus): position independent code for a shared
    /// library (`.so`). Default OFF — the path for programmes and for
    /// Osum stays character for character the one it was.
    static PIC: Cell<bool> = const { Cell::new(false) };
}

/// `--pic`. Moves the tables that hold ABSOLUTE addresses out of the
/// read-only `.rodata` into `.data.rel.ro`, which is writable while the
/// loader relocates and read-only afterwards.
///
/// WHY THIS IS NEEDED, measured and not guessed: a `.so` in which a
/// relocation points into a read-only section carries `TEXTREL` in its
/// dynamic section. Android's loader refuses such a library outright
/// from API 23 on. Measured on `lib/paint/b3_main.fi` for aarch64: 135
/// `R_AARCH64_ABS64` in `.rodata`, all of them jump tables of dense
/// `switch` expressions, plus the tables of the collector, of the
/// interfaces and of the function values.
pub fn pic_set(on: bool) {
    PIC.with(|p| p.set(on));
}

/// Is this compilation position independent?
pub fn pic() -> bool {
    PIC.with(|p| p.get())
}

/// The section for a table that holds addresses. Without `--pic`
/// exactly what stood there before.
pub fn reloc_rodata() -> &'static str {
    if pic() {
        ".section .data.rel.ro"
    } else {
        ".section .rodata"
    }
}

/// `--target=<name>`. `Err` = unknown name.
pub fn flag_set(name: &str) -> Result<(), String> {
    let t = match name {
        "x86_64-linux" | "x86-64-linux" | "x86_64" => Target::X86_64,
        "aarch64-linux" | "arm64-linux" | "aarch64" => Target::Aarch64,
        other => {
            return Err(format!(
                "unknown target '{}' (allowed: x86_64-linux, aarch64-linux)",
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

/// `.align`/`.balign` of the active machine (see `Target::align_directive`).
pub fn align(bytes: u64) -> String {
    active().align_directive(bytes)
}

/// Reset — only for the module tests, which compile several programs in one
/// process (the same reason `prof::reset` exists).
#[cfg(test)]
pub fn reset() {
    ACTIVE.with(|a| a.set(Target::X86_64));
    PIC.with(|p| p.set(false));
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
}
