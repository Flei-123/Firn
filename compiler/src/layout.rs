//! Zwischenschicht zwischen **Feldzugriff** und **Speicherort**
//! (`DESIGNZIELE.md` §8, Fundamentpunkt aus §10.4).
//!
//! # Warum es dieses Modul gibt
//!
//! Heute kennt Firn genau eine Anordnung: **Array von Strukturen** (AoS). Alle
//! Felder eines Wertes liegen zusammenhaengend, die Adresse eines Feldes ist
//! `Basis + Versatz`. Genau diese Gleichsetzung ist aber die Annahme, die
//! **Struktur von Arrays** (SoA, geplanter `SoaVec[T]`) unmoeglich macht: dort
//! existiert der zusammenhaengende Wert physisch gar nicht, jedes Feld hat sein
//! eigenes Array, und die Adresse des Feldes `f` von Element `i` lautet
//! `spalte_f + i * groesse(f)` — nicht `basis + versatz_f`.
//!
//! Solange `a.b` im ganzen Baum fest als „Basis plus Versatz" ausgeschrieben
//! wird, ist SoA nicht nachruestbar, ohne jede Aufrufstelle anzufassen.
//! Deshalb geht **jeder** Feld- und Elementzugriff des Lowerings durch die
//! Funktionen dieses Moduls. Eine zweite Anordnung einzufuehren heisst dann:
//! **hier** eine Fallunterscheidung ergaenzen.
//!
//! # Architekturregel
//!
//! Ausserhalb dieses Moduls berechnet im Lowering niemand `feld.offset` und
//! niemand baut Elementadressen von Hand. `tools/schichten/run.sh` prueft das
//! und ist Teil von `test.sh`.
//!
//! # Was hier (noch) NICHT steht
//!
//! Die SoA-Anordnung selbst. Sie braucht einen Sammlungstyp `SoaVec[T]`,
//! Sichtwerte statt Zeiger und Generics — alles Phase 3/4 der ROADMAP. Dieses
//! Modul ist die **Vorbedingung** dafuer, nicht die Umsetzung.

use crate::diag::Span;
use crate::fir::{BinOp as FBin, FTy, Op, Val};
use crate::lower::Lower;

impl Lower<'_> {
    /// Adresse des Feldes `fname` der Struktur `sidx`, deren Wert an `base`
    /// liegt.
    ///
    /// **Einziger** Weg, an eine Feldadresse zu kommen. Bei AoS ist das
    /// `base + versatz`; bei SoA wuerde hier stattdessen die Spaltenadresse
    /// berechnet.
    pub(crate) fn field_addr(
        &mut self,
        base: Val,
        sidx: usize,
        fname: &str,
        span: Span,
    ) -> Option<Val> {
        let off = match self.info.tcx.structs.get(sidx).and_then(|s| s.field(fname)) {
            Some(f) => f.offset,
            None => return self.ice(span, "unbekanntes feld im lowering"),
        };
        Some(self.field_addr_at(base, off))
    }

    /// Adresse eines Feldes, dessen Versatz bereits bekannt ist.
    ///
    /// Wird von `lower_match.rs` fuer die Nutzdaten einer Aufzaehlungsvariante
    /// gebraucht: dort steht der Versatz in `VariantDef::offsets`, nicht in
    /// einer benannten Feldliste. Auch dieser Weg laeuft bewusst hier durch,
    /// damit es nur **eine** Stelle gibt, an der aus einem Versatz eine Adresse
    /// wird.
    pub(crate) fn field_addr_at(&mut self, base: Val, offset: u64) -> Val {
        self.ptradd_const(base, offset)
    }

    /// Adresse des Elements mit dem **konstanten** Index `index` in einem Feld
    /// von Elementen der Groesse `elem_size` ab `base`.
    ///
    /// Fuer Literale (`[a, b, c]`) und ausgerollte Wiederholungen.
    pub(crate) fn elem_addr_const(&mut self, base: Val, elem_size: u64, index: u64) -> Val {
        self.ptradd_const(base, elem_size * index)
    }

    /// Adresse des Elements mit dem **berechneten** Index `index`.
    ///
    /// `index` wird auf `u64` gebracht, mit der Elementgroesse multipliziert und
    /// auf `base` addiert. Bei SoA waere `base` stattdessen die Spaltenbasis des
    /// jeweiligen Feldes und die Multiplikation liefe pro Feld getrennt.
    pub(crate) fn elem_addr(
        &mut self,
        base: Val,
        elem_size: u64,
        index: Val,
        index_ty: FTy,
    ) -> Val {
        let idx64 = if index_ty == FTy::U64 {
            index
        } else {
            self.push(FTy::U64, Op::Cast { src: index, from: index_ty })
        };
        let sz = self.konst(FTy::U64, elem_size as i128);
        let off = self.push(FTy::U64, Op::Bin(FBin::Mul, idx64, sz));
        self.push(FTy::Ptr, Op::PtrAdd { base, off })
    }
}
