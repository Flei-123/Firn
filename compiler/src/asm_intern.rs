//! **ROUND KODIERER (the encoder) -- the flag.**
//!
//! The old way (assembler text to `as`) stays the default until the counter
//! check over the whole tree is octet-identical. `--asm-intern` switches to
//! the compiler's own encoder. Deliberately a process-wide switch and not an
//! option passed along: it touches exactly one place (`assemble()` in
//! `main.rs`), and a compiler run has exactly one target machine.

use std::cell::Cell;

thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
}

pub fn set(v: bool) {
    ON.with(|c| c.set(v));
}

pub fn get() -> bool {
    ON.with(|c| c.get())
}
