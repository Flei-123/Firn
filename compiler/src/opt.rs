//! Optimierer auf FIR: Konstantenfaltung und Entfernen toten Codes.
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn optimize(m: &mut fir::Module) -> OptStats`
//! Regel: Die Optimierung darf das Programmverhalten NIE aendern. Die
//! Testsuite faehrt jedes Programm mit und ohne `--no-opt` und vergleicht.

use crate::fir::Module;

#[derive(Clone, Copy, Debug, Default)]
pub struct OptStats {
    /// Anzahl zu Konstanten gefalteter Instruktionen
    pub folded: usize,
    /// entfernte Instruktionen (tot/unbenutzt und rein)
    pub removed_insts: usize,
    /// entfernte, unerreichbare Basisbloecke
    pub removed_blocks: usize,
}

/// STUB — wird von Modul "opt" implementiert.
pub fn optimize(m: &mut Module) -> OptStats {
    let _ = m;
    OptStats::default()
}
