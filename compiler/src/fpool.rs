// SPDX-License-Identifier: MPL-2.0
//! **The pool of floating-point constants** (round TEMPO 6).
//!
//! ## Why it exists
//!
//! SSE has no form with an immediate constant. Until now the generator
//! built every floating-point constant at run time:
//!
//! ```text
//!     mov  eax, 0x3f000000
//!     movd xmm10, eax
//!     ...
//!     mulss xmm9, xmm10
//! ```
//!
//! That costs two instructions **and a register** for as long as the constant
//! is needed. In `l3_dct3_9` of the sound decoder these are six constants
//! -- that is, six of the twelve `xmm` that the allocator has to hand out.
//!
//! C does it differently and better: the constant lives in `.rodata` and is the
//! MEMORY OPERAND of the calculation.
//!
//! ```text
//!     mulss xmm8, dword ptr [rip + .Lfc3]
//! ```
//!
//! One instruction, no register. Exactly that is what stands here: one table per
//! translation unit, one entry per bit pattern and width, and the
//! addressing relative to the instruction pointer (`rip`), so that the program may lie
//! anywhere in memory.

use std::cell::RefCell;

thread_local! {
    /// (bit pattern, single precision?) in the order of first occurrence.
    static POOL: RefCell<Vec<(u64, bool)>> = const { RefCell::new(Vec::new()) };
}

/// Forget everything (one translation unit per run).
pub fn reset() {
    POOL.with(|p| p.borrow_mut().clear());
}

/// Gibt es ueberhaupt Eintraege?
pub fn any() -> bool {
    POOL.with(|p| !p.borrow().is_empty())
}

/// Enters the bit pattern (or finds it again) and returns the label.
pub fn intern(bits: u64, single: bool) -> String {
    POOL.with(|p| {
        let mut p = p.borrow_mut();
        let key = (bits, single);
        let idx = match p.iter().position(|e| *e == key) {
            Some(i) => i,
            None => {
                p.push(key);
                p.len() - 1
            }
        };
        label_of(idx)
    })
}

fn label_of(i: usize) -> String {
    format!(".Lfconst{}", i)
}

/// The memory operand for a label -- `rip`-relative, so that the program stays
/// relocatable.
pub fn operand(label: &str, single: bool) -> String {
    format!("{} ptr [rip + {}]", if single { "dword" } else { "qword" }, label)
}

/// The `.rodata` section with all entries.
pub fn rodata_asm() -> String {
    let mut out = String::new();
    POOL.with(|p| {
        let p = p.borrow();
        if p.is_empty() {
            return;
        }
        out.push_str(".section .rodata\n");
        for (i, (bits, single)) in p.iter().enumerate() {
            if *single {
                out.push_str("    .align 4\n");
                out.push_str(&format!("{}:\n", label_of(i)));
                out.push_str(&format!("    .long {}\n", *bits as u32));
            } else {
                out.push_str("    .align 8\n");
                out.push_str(&format!("{}:\n", label_of(i)));
                out.push_str(&format!("    .quad {}\n", bits));
            }
        }
        out.push_str(".text\n");
    });
    out
}
