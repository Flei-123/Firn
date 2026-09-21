// SPDX-License-Identifier: MPL-2.0
//! **Der Vorrat der Gleitzahl-Konstanten** (Runde TEMPO 6).
//!
//! ## Warum es ihn gibt
//!
//! SSE hat keine Form mit unmittelbarer Konstante. Bis hierher baute der
//! Erzeuger jede Gleitzahl-Konstante zur Laufzeit auf:
//!
//! ```text
//!     mov  eax, 0x3f000000
//!     movd xmm10, eax
//!     ...
//!     mulss xmm9, xmm10
//! ```
//!
//! Das kostet zwei Befehle **und ein Register**, solange die Konstante
//! gebraucht wird. In `l3_dct3_9` des Tondekoders sind das sechs Konstanten
//! -- also sechs der zwoelf `xmm`, die der Zuteiler zu vergeben hat.
//!
//! C macht es anders und besser: die Konstante steht in `.rodata` und ist der
//! SPEICHEROPERAND der Rechnung.
//!
//! ```text
//!     mulss xmm8, dword ptr [rip + .Lfc3]
//! ```
//!
//! Ein Befehl, kein Register. Genau das steht hier: eine Tabelle je
//! Uebersetzungseinheit, ein Eintrag je Bitmuster und Breite, und die
//! Adressierung relativ zum Befehlszeiger (`rip`), damit das Programm an
//! jeder Stelle im Speicher liegen darf.

use std::cell::RefCell;

thread_local! {
    /// (Bitmuster, einfach genau?) in der Reihenfolge des ersten Auftretens.
    static POOL: RefCell<Vec<(u64, bool)>> = const { RefCell::new(Vec::new()) };
}

/// Alles vergessen (eine Uebersetzungseinheit je Lauf).
pub fn reset() {
    POOL.with(|p| p.borrow_mut().clear());
}

/// Gibt es ueberhaupt Eintraege?
pub fn any() -> bool {
    POOL.with(|p| !p.borrow().is_empty())
}

/// Traegt das Bitmuster ein (oder findet es wieder) und liefert die Marke.
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

/// Der Speicheroperand fuer eine Marke -- `rip`-relativ, damit das Programm
/// verschieblich bleibt.
pub fn operand(label: &str, single: bool) -> String {
    format!("{} ptr [rip + {}]", if single { "dword" } else { "qword" }, label)
}

/// Der Abschnitt `.rodata` mit allen Eintraegen.
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
