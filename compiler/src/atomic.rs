//! Atomare Lese-Aenderungs-Schreib-Operation (Runde 47) — die Grundlage von
//! `Arc[T]` (SPEC §3.4: „`Arc[T]` ist die fadensichere Variante (atomarer
//! Zaehler). Getrennter Typ, damit einfaediger Code den atomaren Zaehler nicht
//! bezahlt.").
//!
//! Genau EIN Primitiv, absichtlich das kleinste, das reicht:
//!
//! ```firn
//! __atomar_addieren(p: *mut u64, delta: u64) -> u64   // liefert den ALTEN Wert
//! ```
//!
//! Es wird zu einer einzigen Maschineninstruktion `lock xadd qword ptr [..], r`.
//! Damit laesst sich ein Zaehler erhoehen (Ergebnis egal) und ebenso
//! erniedrigen und dabei erkennen, ob man der Letzte war (alter Wert == 1) —
//! mehr braucht Referenzzaehlung nicht. Subtraktion ist die Addition des
//! Zweierkomplements; ein eigenes Primitiv dafuer waere Ballast.
//!
//! **Warum ein eigener Durchgriff bis in den Codegenerator und nicht einfach
//! `*p = *p + d`?** Weil das drei Instruktionen sind (`load`, `add`, `store`)
//! und zwischen ihnen ein anderer Faden dasselbe tun kann — genau die
//! verlorene Erhoehung, gegen die `Arc` existiert. Firn hat in Stufe 0 keine
//! Faeden (SPEC §7), deshalb ist der Unterschied heute NICHT durch einen
//! Zweifaden-Lauf messbar; nachweisbar ist er an der erzeugten Instruktion
//! (`tools/atomic/run.sh` liest den Assembler und verlangt das `lock`-Praefix).
//! Das steht so auch in `docs/RUNDE47.md` — kein „fadensicher" ohne Beleg.
//!
//! **Nicht hier drin:** Speicherordnungen (`acquire`/`release`/`relaxed`),
//! Vergleichs-Tausch (`compare_exchange`), atomare Lasten/Speicherungen
//! kleinerer Breiten. `lock xadd` hat auf x86-64 ohnehin volle Ordnung; die
//! feineren Modelle gehoeren in die Runde, die Faeden bringt.

use crate::ast::Expr;
use crate::diag::Span;
use crate::fir::{FTy, Op, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

/// Name des Primitivs im Quelltext.
pub(crate) const ADD: &str = "__atomic_add";

/// Ist `name` der Name des eingebauten Primitivs?
pub(crate) fn is_atomic_call(name: &str) -> bool {
    name == ADD
}

// ------------------------------------------------------------------- Typphase

/// Hook aus `sema::call`. `None`, wenn es nicht das Primitiv ist oder im
/// Programm eine gleichnamige Funktion steht — die gewinnt dann.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if !is_atomic_call(name) || ck.fns.contains_key(name) {
        return None;
    }
    if args.len() != 2 {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error_note(
            espan,
            format!(
                "'{}' erwartet genau zwei argumente (zeiger, summand), gefunden {}",
                ADD,
                args.len()
            ),
            "die form ist __atomar_addieren(p: *mut u64, delta: u64) -> u64",
        );
        return Some(Type::Error);
    }
    let pt = ck.expr(&args[0], Some(&Type::ptr(Type::U64, true)));
    let dt = ck.expr(&args[1], Some(&Type::U64));
    if !pt.is_error() && !is_u64_ptr(&pt) {
        ck.dg.error_note(
            args[0].span,
            format!(
                "'{}' erwartet als erstes argument einen *mut u64, gefunden {}",
                ADD,
                ck.tcx.name_of(&pt)
            ),
            "atomar geaendert wird genau ein 64-bit-wort",
        );
        return Some(Type::Error);
    }
    if !dt.is_error() && !fits_as_u64(&dt) {
        ck.dg.error(
            args[1].span,
            format!(
                "'{}' erwartet als zweites argument einen u64, gefunden {}",
                ADD,
                ck.tcx.name_of(&dt)
            ),
        );
        return Some(Type::Error);
    }
    let _ = nspan;
    Some(Type::U64)
}

fn is_u64_ptr(t: &Type) -> bool {
    match t {
        Type::Ptr { inner, .. } => **inner == Type::U64,
        _ => false,
    }
}

fn fits_as_u64(t: &Type) -> bool {
    matches!(t, Type::U64 | Type::UntypedInt)
}

// ---------------------------------------------------------------- Lowerphase

/// Hook aus `lower::lower_call`.
pub(crate) fn lower_atomic_call(lo: &mut Lower, name: &str, args: &[Expr], span: Span) -> Option<Option<Val>> {
    let _ = name;
    if args.len() != 2 {
        return lo.ice(span, "atomar-primitiv mit falscher stellenzahl");
    }
    let p = lo.lower_expr(&args[0])?;
    let d = lo.lower_expr(&args[1])?;
    Some(Some(lo.push(FTy::U64, Op::AtomicAdd { addr: p, val: d })))
}
