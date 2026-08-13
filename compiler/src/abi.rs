//! Aufrufkonvention: System V AMD64 (SPEC §13, §14.1).
//!
//! Diese Datei ist die **einzige Wahrheit** darueber, wie ein Wert eine
//! Funktionsgrenze ueberquert. `sema`, `lower`, `codegen_x86` und die Module
//! `types`/`opt` fragen hier nach, statt eigene Regeln zu erfinden.
//!
//! Klassifikation nach System V AMD64 (§3.2.3 der ABI):
//!   * Ganzzahlen, `bool`, Zeiger -> INTEGER, ein Wort
//!   * Aggregate bis 16 Byte      -> INTEGER, ein oder zwei Woerter
//!   * Aggregate ueber 16 Byte    -> MEMORY
//!   * Gleitkomma                 -> SSE (Stufe 0 hat keine Gleitkommatypen)
//!
//! UMSETZUNG (siehe SPEC §14.1 Punkt 1): INTEGER-Woerter werden wie in der ABI
//! in `rdi, rsi, rdx, rcx, r8, r9` und danach auf dem Stapel uebergeben.
//! MEMORY-Argumente werden **als versteckter Zeiger auf eine Kopie des
//! Aufrufers** uebergeben statt als Stapelkopie; Rueckgaben ueber 8 Byte laufen
//! immer ueber den versteckten Zeiger in `rdi` (`rax` liefert ihn zurueck).
//! Beides ist in SPEC §14.1 als bewusste Abweichung festgehalten.

use crate::types::{Type, TypeCtx};

/// Klasse eines Arguments/Rueckgabewertes an der Funktionsgrenze.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgClass {
    /// In Ganzzahlregistern uebergeben; `u8` ist die Anzahl der 8-Byte-Woerter
    /// (0 fuer `()`), hoechstens 2 nach System V.
    Integer(u8),
    /// Ueber Speicher (Stufe 0: versteckter Zeiger auf eine Kopie).
    Memory,
    /// Gleitkomma. Stufe 0 kennt keine Gleitkommatypen; die Variante gehoert
    /// zur vereinbarten Schnittstelle und wird von `classify` nie geliefert.
    #[allow(dead_code)] // Teil der vereinbarten Schnittstelle (PLAN §3.1)
    Sse,
}

/// Groesse der groessten Struktur, die noch in Registern uebergeben wird.
pub const MAX_INTEGER_AGGREGATE: u64 = 16;

/// System-V-Klassifikation eines Quelltyps.
pub fn classify(ty: &Type, tcx: &TypeCtx) -> ArgClass {
    match ty {
        Type::Void | Type::Error => ArgClass::Integer(0),
        Type::Array(..) | Type::Struct(_) => {
            let size = tcx.size_of(ty);
            if size == 0 {
                ArgClass::Integer(0)
            } else if size > MAX_INTEGER_AGGREGATE {
                ArgClass::Memory
            } else {
                ArgClass::Integer(((size + 7) / 8) as u8)
            }
        }
        // Alle skalaren Typen der Stufe 0 sind hoechstens 8 Byte breit.
        _ => ArgClass::Integer(1),
    }
}

/// Ist `ty` ein Aggregat (Struct/Array)?
pub fn is_aggregate(ty: &Type) -> bool {
    matches!(ty, Type::Array(..) | Type::Struct(_))
}

/// Braucht der Rueckgabetyp den versteckten Zeiger (`sret`) in `rdi`?
/// Das gilt fuer jedes Aggregat ueber 8 Byte (SPEC §14.1: Abweichung von
/// System V, das 9..16 Byte in `rax:rdx` zurueckgibt).
pub fn ret_needs_sret(ty: &Type, tcx: &TypeCtx) -> bool {
    is_aggregate(ty) && tcx.size_of(ty) > 8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Type, TypeCtx};

    fn ctx_with(fields: Vec<(String, Type)>) -> (TypeCtx, Type) {
        let mut tcx = TypeCtx::new();
        let i = tcx.declare("S");
        tcx.set_fields(i, fields);
        (tcx, Type::Struct(i))
    }

    #[test]
    fn skalare_belegen_ein_wort() {
        let tcx = TypeCtx::new();
        for t in [Type::I8, Type::U64, Type::Bool, Type::ptr(Type::I32, true)] {
            assert_eq!(classify(&t, &tcx), ArgClass::Integer(1), "{:?}", t);
        }
        assert_eq!(classify(&Type::Void, &tcx), ArgClass::Integer(0));
    }

    #[test]
    fn kleine_structs_in_registern() {
        let (tcx, s) = ctx_with(vec![("a".into(), Type::I32), ("b".into(), Type::I32)]);
        assert_eq!(tcx.size_of(&s), 8);
        assert_eq!(classify(&s, &tcx), ArgClass::Integer(1));
        assert!(!ret_needs_sret(&s, &tcx));

        let (tcx2, s2) = ctx_with(vec![("a".into(), Type::I64), ("b".into(), Type::I64)]);
        assert_eq!(tcx2.size_of(&s2), 16);
        assert_eq!(classify(&s2, &tcx2), ArgClass::Integer(2));
        assert!(ret_needs_sret(&s2, &tcx2));
    }

    #[test]
    fn grosse_structs_ueber_speicher() {
        let (tcx, s) = ctx_with(vec![
            ("a".into(), Type::I64),
            ("b".into(), Type::I64),
            ("c".into(), Type::I64),
        ]);
        assert_eq!(tcx.size_of(&s), 24);
        assert_eq!(classify(&s, &tcx), ArgClass::Memory);
        assert!(ret_needs_sret(&s, &tcx));
    }

    #[test]
    fn arrays_zaehlen_wie_structs() {
        let tcx = TypeCtx::new();
        let a = Type::Array(Box::new(Type::U8), 12);
        assert_eq!(classify(&a, &tcx), ArgClass::Integer(2));
        let b = Type::Array(Box::new(Type::U8), 17);
        assert_eq!(classify(&b, &tcx), ArgClass::Memory);
        let c = Type::Array(Box::new(Type::U8), 3);
        assert_eq!(classify(&c, &tcx), ArgClass::Integer(1));
    }
}
