// SPDX-License-Identifier: GPL-2.0-only
//! **ROUND 89** — global variables: `static` and `static mut` (SPEC §14.1
//! item 5).
//!
//! INTERFACE (fixed):
//!   `pub fn reset()`
//!   `pub fn register(name: &str, mutable: bool, bytes: Vec<u8>, align: u64)`
//!   `pub fn label_of(name: &str) -> String`
//!   `pub fn any() -> bool`
//!   `pub fn data_asm() -> String`
//!
//! ## Why this file exists at all
//!
//! Until this round Firn had `const` and nothing else. SPEC §14.1 item 5
//! named the three questions that had to be answered before a data section
//! could exist, and this round answers them:
//!
//! * **Initialisation order.** There is none, because there is nothing to
//!   order: the initialiser of a `static` must be evaluable AT COMPILE TIME.
//!   What lands in the object file is a finished sequence of octets, not a
//!   program that has to run before `main`. That removes the whole class of
//!   "static initialisation order fiasco" bugs by construction rather than
//!   by discipline. `comptime` (round 12, SPEC §6.4) is what makes this
//!   affordable — the language already had the evaluator.
//! * **The collector.** A `static Gc[T]` is **refused**, with a message that
//!   says why (`sema.rs::check_statics`). The root set of the collector is
//!   the stack plus the callee-saved registers (SPEC §3.5.3); a data section
//!   entry is neither, and making it one means teaching the collector to walk
//!   a second, differently shaped root list. Claiming to support it while the
//!   collector cannot see it would produce a use-after-free that no test
//!   would catch reliably. The honest answer is the refusal, and it costs the
//!   kernel nothing: the `kernel` profile has no collector at all (SPEC §2).
//! * **Threads.** A `static mut` is shared mutable state, and Firn has had
//!   real threads since round 49. The rule chosen here is the one a kernel
//!   can live with, because a kernel is the reason this round exists:
//!   **access is not protected and the compiler does not pretend otherwise**
//!   (SPEC §14.1.statics). Reading is plain; writing is plain too. What
//!   Firn gives you to make it safe are the atomic primitives of rounds
//!   47/49 (`atomic_add`, `atomic_cas`) and `Mutex` — used by hand, at the
//!   place where the sharing actually happens. Forbidding the plain write
//!   would have made `static mut` useless for exactly the code that needs
//!   it (an interrupt table, a scheduler's run queue, a page allocator's
//!   free list), all of which serialise access with their own lock or with
//!   interrupts disabled, and none of which can express that to a compiler
//!   that has no ownership system yet.
//!
//! ## Which section
//!
//! | `static` | contents | section |
//! |---|---|---|
//! | `mut` | all octets zero | `.bss` (`.zero n`, no space in the file) |
//! | `mut` | anything else | `.data` |
//! | not `mut` | anything | `.rodata` (the loader maps it read-only) |
//!
//! An immutable all-zero `static` deliberately does NOT go to `.bss`:
//! `.bss` is writable, and putting a value there that the program promised
//! not to change would give away the one guarantee the missing `mut` buys.
//!
//! ## The name in the object file
//!
//! `.Lstatic_<name>`, where `<name>` is the name AFTER the module system has
//! mangled it (`modules.rs::mangle`, `vec__COUNT`). A local label is enough
//! because every module of one program is merged into ONE `fir::Module` and
//! therefore into one object file (SPEC §14.1 item 15) — there is no second
//! translation unit that could need to resolve it.

use std::cell::RefCell;

/// One global variable, ready to be written out.
pub struct StaticInfo {
    /// Name after module mangling.
    pub name: String,
    pub mutable: bool,
    /// The finished initial value, little endian, exactly `size_of(ty)` long.
    pub bytes: Vec<u8>,
    pub align: u64,
}

thread_local! {
    static TABLE: RefCell<Vec<StaticInfo>> = const { RefCell::new(Vec::new()) };
}

/// Empties the table — between compilations of the SAME process (module
/// tests, `--package`), exactly like `panic_rt::reset`.
pub fn reset() {
    TABLE.with(|t| t.borrow_mut().clear());
}

/// Records one `static`. The order of the calls is the order of the
/// declarations, and that is the order the data section comes out in — two
/// runs of the same compiler on the same source produce the same octets.
pub fn register(name: &str, mutable: bool, bytes: Vec<u8>, align: u64) {
    TABLE.with(|t| {
        let mut t = t.borrow_mut();
        if t.iter().any(|s| s.name == name) {
            return;
        }
        t.push(StaticInfo {
            name: name.to_string(),
            mutable,
            bytes,
            align: align.max(1),
        });
    });
}

/// The assembler label of a `static`.
pub fn label_of(name: &str) -> String {
    format!(".Lstatic_{}", name.replace('#', "."))
}

/// Does this program have a `static` at all? Only then is a data section
/// written; a program without one carries not one byte of this.
pub fn any() -> bool {
    TABLE.with(|t| !t.borrow().is_empty())
}

/// Is `name` a registered `static`? (`escape.rs` asks: the address of a
/// global may leave a frame, unlike the address of a local.)
pub fn is_static(name: &str) -> bool {
    TABLE.with(|t| t.borrow().iter().any(|s| s.name == name))
}

fn p2align(a: u64) -> u32 {
    let mut k = 0;
    let mut v = 1u64;
    while v < a && k < 16 {
        v <<= 1;
        k += 1;
    }
    k
}

fn bytes_asm(out: &mut String, bytes: &[u8]) {
    for row in bytes.chunks(16) {
        out.push_str("    .byte ");
        for (i, b) in row.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&b.to_string());
        }
        out.push('\n');
    }
}

/// `.bss` / `.data` / `.rodata` for every `static` of this program, in
/// declaration order. Identical text on x86-64 and on aarch64 — a data
/// section has no instructions in it, so there is nothing per-machine to
/// decide here.
pub fn data_asm() -> String {
    let mut out = String::new();
    TABLE.with(|t| {
        let t = t.borrow();
        if t.is_empty() {
            return;
        }
        for s in t.iter() {
            let zero = s.bytes.iter().all(|b| *b == 0);
            let section = if !s.mutable {
                ".section .rodata"
            } else if zero {
                // ROUND WINDOWS: `@nobits` and the flag letters are ELF's
                // spelling. COFF says the same thing with `b` (uninitialised
                // data) and `w` (writable) -- and it means the same thing:
                // the octets take up no room in the file.
                if crate::target::windows() {
                    ".section .bss,\"bw\""
                } else {
                    ".section .bss,\"aw\",@nobits"
                }
            } else {
                ".section .data"
            };
            out.push_str(section);
            out.push('\n');
            out.push_str(&format!("    .p2align {}\n", p2align(s.align)));
            out.push_str(&format!("{}:\n", label_of(&s.name)));
            if s.mutable && zero {
                out.push_str(&format!("    .zero {}\n", s.bytes.len().max(1)));
            } else if s.bytes.is_empty() {
                out.push_str("    .zero 1\n");
            } else {
                bytes_asm(&mut out, &s.bytes);
            }
        }
        out.push_str(".text\n");
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_mutable_goes_to_bss() {
        reset();
        register("COUNT", true, vec![0; 8], 8);
        let s = data_asm();
        assert!(s.contains(".section .bss"), "{}", s);
        assert!(s.contains(".zero 8"), "{}", s);
        reset();
    }

    #[test]
    fn value_and_mutable_goes_to_data() {
        reset();
        register("N", true, vec![7, 0, 0, 0], 4);
        let s = data_asm();
        assert!(s.contains(".section .data"), "{}", s);
        assert!(s.contains(".byte 7, 0, 0, 0"), "{}", s);
        reset();
    }

    #[test]
    fn immutable_goes_to_rodata_even_when_zero() {
        reset();
        register("T", false, vec![0; 4], 4);
        let s = data_asm();
        assert!(s.contains(".section .rodata"), "{}", s);
        assert!(!s.contains(".section .bss"), "{}", s);
        reset();
    }
}
