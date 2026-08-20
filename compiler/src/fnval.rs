//! **Round 58** — functions as first class values: function pointers and
//! closures (`docs/ROUND58.md`).
//!
//! ## The representation, binding for both compilers
//!
//! A value of type `fn(A, B) -> R` is **one machine word**: the address of a
//! FUNCTION RECORD.
//!
//! ```text
//! record:  [0]      address of the machine code
//!          [8+8*k]  the k-th captured value of a closure
//! ```
//!
//! * A named function taken as a value gets a record of exactly one word in
//!   `.rodata` (`.L__fnv.<symbol>`). It costs no allocation and works
//!   without the collector, under `profile kernel` and inside `#[no_gc]`.
//! * A closure that captures nothing gets the same kind of static record.
//! * A closure that captures something needs storage for the captured
//!   values. That storage is a GC object of a synthesised class
//!   (`gc.rs::declare_closure_class`), so the collector traces the captured
//!   `Gc[T]` pointers through the ordinary type table — no special path, no
//!   pinning, no external root.
//!
//! ## The call
//!
//! ```text
//! %c = load.ptr [%f]              ; the code address out of word 0
//! %r = calli.R  %c(a, b, %f)      ; the record goes in as the LAST argument
//! ```
//!
//! The record travels as the last argument, which is why a named function
//! needs no shim: System V lets the caller pass one argument too many, and a
//! function that does not know about it never reads it. A closure body is
//! translated as an ordinary function whose last parameter is the record —
//! that is where it reads its captured values from.
//!
//! **A direct call stays direct.** `add(1, 2)` is still `Op::Call` and thus
//! `call add`; the indirection arises only where the target really sits in a
//! value (proof: `tools/fnval/run.sh`).

use std::cell::RefCell;

/// Prefix of every function record in `.rodata`. It is file local (`.L`) and
/// contains a dot, so it can never collide with a symbol of the source text.
const RECORD_LABEL: &str = ".L__fnv.";

#[derive(Default)]
struct Registry {
    /// Symbol names for which a static record has to be emitted, in the
    /// order in which they were first needed (deterministic output).
    records: Vec<String>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

/// Registers a static record for the function `name` and yields its key.
pub(crate) fn record_of(name: &str) -> String {
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        if !reg.records.iter().any(|x| x == name) {
            reg.records.push(name.to_string());
        }
    });
    name.to_string()
}

pub(crate) fn has_records() -> bool {
    REG.with(|r| !r.borrow().records.is_empty())
}

/// Assembler name of a function record.
pub(crate) fn record_label(key: &str) -> String {
    format!("{}{}", RECORD_LABEL, crate::codegen_x86::label(key))
}

/// All static function records as one `.rodata` block.
pub(crate) fn records_asm() -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let recs: Vec<String> = REG.with(|r| r.borrow().records.clone());
    if recs.is_empty() {
        return out;
    }
    let _ = writeln!(out, ".section .rodata");
    let _ = writeln!(out, ".align 8");
    for k in recs {
        let sym = crate::codegen_x86::label(&k);
        let _ = writeln!(out, "{}{}:", RECORD_LABEL, sym);
        let _ = writeln!(out, "    .quad {}", sym);
    }
    out
}
