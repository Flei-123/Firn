//! **RUNDE KODIERER — die Fahne.**
//!
//! Runde KODIERER hat den eigenen Binaerkodierer gebaut, Runde KODIERER II
//! die DWARF-Zeilentabelle dazu. Damit ist der letzte Grund weg, `as` zu
//! rufen: der eigene Weg ist ueber den ganzen Baum oktettgleich, und das
//! schliesst `.debug_line`, `.debug_info`, `.debug_abbrev` und
//! `.debug_aranges` ein (`tools/kodierer/run.sh`).
//!
//! Also ist der EIGENE Weg jetzt die Vorgabe. `--asm-extern` schaltet
//! zurueck auf `as` — als Rueckfallebene, wenn eine Objektdatei je
//! verdaechtig aussieht, und als Vergleichsmass fuer die Abnahme.
//! `--asm-intern` bleibt erlaubt und tut nichts weiter, als das zu sagen,
//! was ohnehin gilt; die Werkzeuge der Abnahme schreiben es hin, damit man
//! ihnen ansieht, was sie pruefen.
//!
//! Bewusst ein Prozess-weiter Schalter und keine durchgereichte Option: er
//! beruehrt genau eine Stelle (`assemble()` in `main.rs`), und ein
//! Uebersetzerlauf hat genau einen Zielrechner.

use std::cell::Cell;

thread_local! {
    static ON: Cell<bool> = const { Cell::new(true) };
}

pub fn set(v: bool) {
    ON.with(|c| c.set(v));
}

pub fn get() -> bool {
    ON.with(|c| c.get())
}
