//! **RUNDE KODIERER — die Fahne.**
//!
//! Der alte Weg (Assemblertext an `as`) bleibt Vorgabe, bis die Gegenprobe
//! ueber den ganzen Baum oktettgleich ist. `--asm-intern` schaltet auf den
//! eigenen Kodierer um. Bewusst ein Prozess-weiter Schalter und keine
//! durchgereichte Option: er beruehrt genau eine Stelle (`assemble()` in
//! `main.rs`), und ein Uebersetzerlauf hat genau einen Zielrechner.

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
