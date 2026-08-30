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
//!
//! ---
//!
//! **Round ANDROID — the third target.**
//!
//! `aarch64-linux-android` is NOT `aarch64-linux-gnu` with a different
//! name. The instruction set is the same, so the whole code generator
//! (`codegen_a64.rs`) and the whole calling convention are shared — but
//! everything AROUND the code differs:
//!
//!   * the C library is Bionic, not glibc,
//!   * the start files are `crtbegin_dynamic.o` / `crtend_android.o`
//!     (executable) resp. `crtbegin_so.o` / `crtend_so.o` (library),
//!   * the dynamic loader is `/system/bin/linker64`,
//!   * position independence is not an option but a requirement — Android
//!     has refused a non-PIE executable since Android 5,
//!   * an API level has to be chosen, because Bionic's set of functions
//!     grew with the releases,
//!   * and the form an app actually loads is a SHARED LIBRARY, not an
//!     executable.
//!
//! Because of that the target enum grew a third variant and a second
//! question was added next to it: not only WHICH machine, but WHICH FORM
//! (`Form::Exe` / `Form::Shared`). `arch()` answers what the code generator
//! wants to know and stays two-valued — no backend had to learn a third
//! machine, and that is the point: the third target costs no instruction.
//!
//! Where the NDK lives, which API level is used and what the linker command
//! looks like is not here but in `android.rs`.

use std::cell::Cell;

/// The instruction set. TWO of them, and that does not change with the
/// third target: `aarch64-linux-gnu` and `aarch64-linux-android` produce
/// the same machine code out of the same FIR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    X86_64,
    Aarch64,
    /// Round ANDROID: aarch64 + Bionic + PIE + NDK linker.
    Aarch64Android,
}

/// What is being produced. Round ANDROID: an app loads a `.so`, not an
/// executable, and the two are linked differently from the very same
/// object file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// A program with an entry point (default, everything before this round).
    Exe,
    /// A shared library (`-shared`): no entry point, exported functions.
    Shared,
}

impl Target {
    /// The name as it is written on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Target::X86_64 => "x86_64-linux",
            Target::Aarch64 => "aarch64-linux",
            Target::Aarch64Android => "aarch64-linux-android",
        }
    }
    /// The instruction set — the only thing a code generator has to know.
    pub fn arch(self) -> Arch {
        match self {
            Target::X86_64 => Arch::X86_64,
            Target::Aarch64 | Target::Aarch64Android => Arch::Aarch64,
        }
    }
    /// Android? Everything that is different about the third target hangs
    /// off this one question.
    pub fn is_android(self) -> bool {
        matches!(self, Target::Aarch64Android)
    }
    /// The assembler for this machine.
    ///
    /// Android too: an object file for `aarch64-linux-android` and one for
    /// `aarch64-linux-gnu` are the same ELF relocatable — the difference is
    /// made by the LINKER, not by the assembler. `binutils-aarch64-linux-gnu`
    /// therefore serves both, and the NDK is only needed for linking.
    pub fn assembler(self) -> &'static str {
        match self {
            Target::X86_64 => "as",
            Target::Aarch64 | Target::Aarch64Android => "aarch64-linux-gnu-as",
        }
    }
    /// The arguments the assembler needs in front of `-o`.
    pub fn as_flags(self) -> &'static [&'static str] {
        match self {
            Target::X86_64 => &["--64"],
            Target::Aarch64 | Target::Aarch64Android => &[],
        }
    }
    /// The linker for this machine.
    pub fn linker(self) -> &'static str {
        match self {
            Target::X86_64 => "ld",
            Target::Aarch64 | Target::Aarch64Android => "aarch64-linux-gnu-ld",
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
    static FORM: Cell<Form> = const { Cell::new(Form::Exe) };
}

/// `--target=<name>`. `Err` = unknown name.
pub fn flag_set(name: &str) -> Result<(), String> {
    let t = match name {
        "x86_64-linux" | "x86-64-linux" | "x86_64" => Target::X86_64,
        "aarch64-linux" | "arm64-linux" | "aarch64" => Target::Aarch64,
        // Round ANDROID. `arm64-v8a` is the name the same machine carries
        // in an Android build file, so it is accepted as well.
        "aarch64-linux-android" | "aarch64-android" | "arm64-v8a" | "android-arm64" => {
            Target::Aarch64Android
        }
        other => {
            return Err(format!(
                "unknown target '{}' (allowed: x86_64-linux, aarch64-linux, aarch64-linux-android)",
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

/// `--shared`. Round ANDROID.
pub fn form_set(f: Form) {
    FORM.with(|x| x.set(f));
}

/// What is produced (`Form::Exe` unless `--shared` was given).
pub fn form() -> Form {
    FORM.with(|x| x.get())
}

/// Short form of `form() == Form::Shared` — asked at three places in the
/// code generator.
pub fn shared() -> bool {
    form() == Form::Shared
}

/// `.align`/`.balign` of the active machine (see `Target::align_directive`).
pub fn align(bytes: u64) -> String {
    active().align_directive(bytes)
}

/// **ROUND ANDROID — where read-only data goes.**
///
/// Every table this compiler emits that is not meant to be written lives in
/// `.rodata`: the jump tables of a `switch`, the type table of the
/// collector, the method tables of the interfaces, the function records,
/// the panic messages, the immutable `static`s. Some of them contain
/// ADDRESSES (`.quad .Lsomething`), and that is the whole problem.
///
/// In a fixed-address executable an address is known at link time and the
/// linker simply writes it down. In a POSITION INDEPENDENT one it is not:
/// the loader has to add the load address to every one of those words at
/// start-up. If the word sits in a read-only section, that produces a TEXT
/// RELOCATION — and Android's loader does not warn about `DT_TEXTREL`, it
/// REFUSES the file:
///
/// ```text
/// CANNOT LINK EXECUTABLE: text relocations (DT_TEXTREL) found in 64-bit ELF file
/// ```
///
/// That is what 105 of the 309 cases of `tools/android/run.sh` died of
/// before this function existed — every program with a garbage collected
/// value or a `switch` over more than a handful of keys.
///
/// `.data.rel.ro` is the section that exists for exactly this: writable
/// while the loader relocates, then mapped read-only by `-z relro` (which
/// `android.rs` passes). The promise of an immutable `static` is kept, the
/// loader is happy, and nothing about the other two targets changes — they
/// are not position independent and their answer is the same `.rodata` it
/// always was.
pub fn rodata_section() -> &'static str {
    if active().is_android() {
        ".section .data.rel.ro,\"aw\",%progbits"
    } else {
        ".section .rodata"
    }
}

/// Reset — only for the module tests, which compile several programs in one
/// process (the same reason `prof::reset` exists).
#[cfg(test)]
pub fn reset() {
    ACTIVE.with(|a| a.set(Target::X86_64));
    FORM.with(|x| x.set(Form::Exe));
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

    /// Round ANDROID: the third target is a target of its own, and it is
    /// NOT the gnu one under another name.
    #[test]
    fn android_is_a_third_target_and_not_the_gnu_one() {
        reset();
        flag_set("aarch64-linux-android").unwrap();
        assert_eq!(active(), Target::Aarch64Android);
        assert_ne!(active(), Target::Aarch64);
        assert_eq!(active().name(), "aarch64-linux-android");
        assert!(active().is_android());
        assert!(!Target::Aarch64.is_android());
        assert!(!Target::X86_64.is_android());
        reset();
    }

    /// ...and it is the SAME machine: same assembler, same directives, same
    /// code generator. That is why the round costs no instruction.
    #[test]
    fn android_shares_the_aarch64_machine() {
        assert_eq!(Target::Aarch64Android.arch(), Arch::Aarch64);
        assert_eq!(Target::Aarch64Android.arch(), Target::Aarch64.arch());
        assert_eq!(
            Target::Aarch64Android.assembler(),
            Target::Aarch64.assembler()
        );
        assert_eq!(Target::Aarch64Android.align_directive(8), ".balign 8");
        assert_eq!(Target::X86_64.arch(), Arch::X86_64);
    }

    #[test]
    fn the_android_spellings_of_the_same_machine() {
        for n in [
            "aarch64-linux-android",
            "aarch64-android",
            "arm64-v8a",
            "android-arm64",
        ] {
            reset();
            flag_set(n).unwrap();
            assert_eq!(active(), Target::Aarch64Android, "{}", n);
        }
        reset();
    }

    /// The read-only data of a position independent artifact may not stay
    /// in `.rodata` when it contains addresses — and on the other two
    /// targets the answer must not have moved a character.
    #[test]
    fn android_puts_relocatable_read_only_data_into_data_rel_ro() {
        reset();
        assert_eq!(rodata_section(), ".section .rodata");
        flag_set("aarch64-linux").unwrap();
        assert_eq!(rodata_section(), ".section .rodata");
        flag_set("aarch64-linux-android").unwrap();
        assert!(rodata_section().starts_with(".section .data.rel.ro"));
        assert!(rodata_section().contains("\"aw\""));
        reset();
        assert_eq!(rodata_section(), ".section .rodata");
    }

    #[test]
    fn the_form_defaults_to_an_executable() {
        reset();
        assert_eq!(form(), Form::Exe);
        assert!(!shared());
        form_set(Form::Shared);
        assert!(shared());
        reset();
        assert!(!shared());
    }
}
