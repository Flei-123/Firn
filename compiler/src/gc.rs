//! Opt-in-Tracing-GC — SPEC §3.5 (`S2`–`S6`), Vererbung §4.4.
//!
//! Diese Datei gehoert dem Modul `gckern` (siehe PLAN.md, Runde „Haertetest 2").
//! Sie enthaelt — sobald gebaut —
//!  * die Parser-Erweiterungen fuer `gc class Name [extends Basis] { … }`
//!    (angebunden ueber `// HOOK gc`-Zeilen in `parser.rs`),
//!  * die Registrierung der Klassen, ihres Feldlayouts, ihrer Typkennung und
//!    ihrer Ahnenkette,
//!  * die Typpruefung von `Gc[T]`, `GcWeak[T]`, `gc Name{…}`, `weak(x)`,
//!    `stark(w)`, `x.as?[T]` und der kostenlosen Aufwaertsumwandlung.
//!
//! Das Lowering nach FIR steht in `gc_lower.rs`, die Sammler-Laufzeit in
//! `lib/gc/`.
//!
//! ## Vertrag nach aussen (stabil, andere Module haengen daran)
//!
//! `nogc.rs` (Modul `nogc`, `#[no_gc]`-Pruefung nach SPEC §3.5.4) benutzt
//! ausschliesslich die drei Abfragen unten. Sie liefern in der Skelettfassung
//! `false` — die Pruefung ist damit nicht falsch, sondern nur noch nicht
//! vollstaendig, und beide Module lassen sich unabhaengig voneinander bauen.

use crate::types::Type;

/// Ist `name` der Aufrufname einer GC-Allokation oder einer Sammler-Funktion,
/// die einen Sammellauf ausloesen kann (`gc Name{…}`, `gc_collect`, …)?
///
/// Genau diese Aufrufe sind in einer `#[no_gc]`-Funktion verboten.
pub(crate) fn ist_gc_alloc_aufruf(_name: &str) -> bool {
    false
}

/// Ist `t` ein GC-Zeigertyp (`Gc[T]` oder `GcWeak[T]`)? Das Schreiben in ein
/// Feld dieses Typs braucht die Einfuegebarriere und ist in `#[no_gc]`
/// verboten.
pub(crate) fn ist_gc_zeiger(_t: &Type) -> bool {
    false
}
