//! Opt-in-Tracing-GC — SPEC §3.5 (`S2`–`S6`), Vererbung §4.4.
//!
//! Diese Datei gehoert dem Modul `gckern` (siehe PLAN.md, Runde „Haertetest 2").
//! Sie enthaelt
//!  * die Parser-Erweiterungen fuer `gc class Name [extends Basis] { … }`
//!    (angebunden ueber `// HOOK gc`-Zeilen in `parser.rs`),
//!  * die Registrierung der Klassen, ihres Feldlayouts, ihrer Typkennung und
//!    ihrer Ahnenkette,
//!  * die Typpruefung von `Gc[T]`, `GcWeak[T]`, `gc Name{…}`, `weak(x)`,
//!    `stark(w)`, `x.as?[T]` und der kostenlosen Aufwaertsumwandlung,
//!  * die Typtabelle fuer den Sammler (`.rodata`, `typtabelle_asm`).
//!
//! Das Lowering nach FIR steht in `gc_lower.rs`, die Sammler-Laufzeit als
//! lesbares Firn in `lib/gc/gc.fi`. Diese Laufzeit wird per `include_str!`
//! eingebettet und automatisch als zusaetzliches Modul eingezogen, sobald im
//! Programm ein `gc class` vorkommt (`laufzeit_quelle`, `modules.rs`).
//!
//! ## Darstellung (verbindlich, `docs/GC.md`)
//!
//! ```text
//! gc class C { … }   -> Struct "gc C" in types::TypeCtx (Praefixlayout der Basis)
//! Gc[C]              -> *mut <Struct "gc C">          (erstklassiger Zeiger)
//! GcWeak[C]          -> Struct "GcWeak[C]" { __p: u64, __s: u64 }
//! gc C{ … }          -> AllocError!Gc[C]
//! ```
//!
//! `GcWeak[C]` traegt den Zeiger **verschleiert** (`__p = p ^ WEAK_XOR`), damit
//! der konservative Stapelscan ihn nicht als starke Wurzel liest, und die
//! Seriennummer `__s` des Zielblocks, damit ein wiederverwendeter Block nicht
//! als altes Ziel durchgeht.
//!
//! ## Vertrag nach aussen (stabil, andere Module haengen daran)
//!
//! `nogc.rs` (Modul `nogc`, `#[no_gc]`-Pruefung nach SPEC §3.5.4) benutzt
//! ausschliesslich die beiden Abfragen `ist_gc_alloc_aufruf` und
//! `ist_gc_zeiger`. Ihre Signaturen sind fest.

use std::cell::RefCell;

use crate::ast::{Expr, ExprKind, TypeExpr};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::Type;

// ---------------------------------------------------------------- Namensraum

/// Praefix aller compilerinternen GC-Namen. Enthaelt `#`, kann also nie aus
/// einem Bezeichner des Quelltextes entstehen.
/// `gc C{…}` steht als Aufruf `"gc C"` im AST. Der Name enthaelt ein
/// Leerzeichen, kann also nie ein Bezeichner des Quelltextes sein — und er
/// liest sich in der `#[no_gc]`-Meldung genauso, wie er im Quelltext steht.
const P_NEU: &str = "gc ";
const P_TYP: &str = "__gc#p:";
const P_WTYP: &str = "__gc#w:";
/// Dieselben Praefixe fuer `mono.rs` (Runde 53: `Gc[T]` in einer Vorlage).
pub(crate) const P_TYP_PUB: &str = P_TYP;
pub(crate) const P_WTYP_PUB: &str = P_WTYP;
const P_AS: &str = "__gc#as:";

/// Aufrufnamen der Laufzeit (`lib/gc/gc.fi`), die einen Sammellauf ausloesen
/// koennen oder den Zustand des Sammlers anfassen.
const LAUFZEIT_SAMMELT: [&str; 4] = ["gc_init", "gc_collect", "__gc_alloc_raw", "__gc_collect_now"];
/// Weitere Laufzeitnamen: reine Abfragen, aber Teil des Sammlers.
const LAUFZEIT_ABFRAGE: [&str; 11] = [
    "gc_set_max_bytes",
    "gc_max_bytes",
    "gc_total_bytes",
    "gc_collections",
    "gc_live_objects",
    "gc_heap_bytes",
    "gc_live_bytes",
    "gc_pause_ns_last",
    "gc_pause_ns_max",
    "gc_pause_ns_total",
    "gc_barriers",
];

/// Compilerintrinsics, die `gc_lower.rs` zu `Op::GcAddr` macht.
pub(crate) const INTR_STATE: &str = "__gc_state";
pub(crate) const INTR_REGS: &str = "__gc_save_regs";

/// Laufzeitfunktion hinter `weak(g)`.
pub(crate) const FN_WEAK: &str = "__gc_weak_raw";
/// Laufzeitfunktion hinter `stark(w)`.
pub(crate) const FN_STARK: &str = "__gc_strong_raw";
/// Laufzeitfunktion hinter `x.as?[T]`.
pub(crate) const FN_AS: &str = "__gc_as_raw";
/// Laufzeitfunktion hinter `gc C{…}`.
pub(crate) const FN_ALLOC: &str = "__gc_alloc_raw";
/// Einfuegebarriere beim Schreiben eines Gc-Zeigers in den Heap.
pub(crate) const FN_BARRIER: &str = "__gc_barrier";
/// Fehlermenge der fehlbaren Allokation (DESIGNZIELE §2).
pub(crate) const ERR_SET: &str = "AllocError";
/// **Runde 47** — Verteiler der Finalisierer (`SPEC` §3.5.3 `S4`).
///
/// Die Laufzeit ruft beim Einsammeln `__gc_finalisiere(art, p)`. Stufe 0 hat
/// keine Funktionszeiger; eine Verteilerfunktion mit einer Kennung ist die
/// ehrliche Entsprechung und braucht keinen indirekten Aufruf im
/// Codegenerator (den baut R46 fuer Vtables, nicht diese Runde).
///
/// Deklariert die **Wurzeldatei** des Programms diese Funktion selbst, nimmt
/// der Compiler sie; sonst legt er die leere Voreinstellung dazu. Nur die
/// Wurzeldatei zaehlt, weil in einem Modul der Name zu `modul__…` wird und
/// die Laufzeit ihn dann nicht mehr faende.
pub(crate) const FN_FINAL: &str = "__gc_finalisiere";

// ---------------------------------------------------------------- Datenmodell

#[derive(Clone, Debug)]
struct Feld {
    name: String,
    ty: TypeExpr,
    span: Span,
}

#[derive(Clone, Debug)]
struct Klasse {
    name: String,
    span: Span,
    basis: Option<(String, Span)>,
    felder: Vec<Feld>,
    /// Index in `types::TypeCtx` (Name `"gc C"`), `usize::MAX` bis zur Anmeldung
    struct_idx: usize,
    /// Index des Structs `GcWeak[C]`
    weak_idx: usize,
    /// Typkennung, ab 1 in Deklarationsreihenfolge
    tid: u64,
    /// nach der Anmeldung: Groesse in Bytes
    size: u64,
    /// Offsets der `Gc[T]`-Felder (praezise Heap-Verfolgung)
    stark_offs: Vec<u64>,
    /// Offsets der `GcWeak[T]`-Felder (nur fuer die Statistik/Dokumentation)
    weak_offs: Vec<u64>,
    /// Typkennung der Basis, 0 = keine
    basis_tid: u64,
    /// Struct-Index der Fehlerunion `AllocError!Gc[C]` (`usize::MAX` = noch nicht)
    union_idx: usize,
    /// Layout ist angemeldet
    fertig: bool,
}

#[derive(Default)]
struct Registry {
    klassen: Vec<Klasse>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

fn index_von(name: &str) -> Option<usize> {
    REG.with(|r| r.borrow().klassen.iter().position(|k| k.name == name))
}

/// Ist `name` ein deklariertes `gc class`?
pub(crate) fn ist_klasse(name: &str) -> bool {
    index_von(name).is_some()
}

/// Gibt es ueberhaupt ein `gc class` in dieser Uebersetzung?
pub(crate) fn hat_klassen() -> bool {
    REG.with(|r| !r.borrow().klassen.is_empty())
}

/// Setzt die Registrierung zurueck (eine je Uebersetzung, `parser::reset_hooks`).
pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

// ------------------------------------------------------- Vertrag fuer nogc.rs

/// Ist `name` der Aufrufname einer GC-Allokation oder einer Sammler-Funktion,
/// die einen Sammellauf ausloesen kann (`gc Name{…}`, `gc_collect`, …)?
///
/// Genau diese Aufrufe sind in einer `#[no_gc]`-Funktion verboten.
pub(crate) fn ist_gc_alloc_aufruf(name: &str) -> bool {
    if name.starts_with(P_NEU) {
        return true;
    }
    // Auch der Modulpfad davor zaehlt (`modul__gc_collect`), sonst waere die
    // Zusage ueber eine Modulgrenze hinweg zu umgehen.
    let blank = name.rsplit("__").next().unwrap_or(name);
    LAUFZEIT_SAMMELT.contains(&name)
        || LAUFZEIT_SAMMELT.contains(&blank)
        || LAUFZEIT_ABFRAGE.contains(&name)
        || LAUFZEIT_ABFRAGE.contains(&blank)
        || name == INTR_STATE
        || name == INTR_REGS
        || name == FN_WEAK
        || name == FN_STARK
        || name == FN_AS
        || name == FN_BARRIER
}

/// Ist `t` ein GC-Zeigertyp (`Gc[T]` oder `GcWeak[T]`)? Das Schreiben in ein
/// Feld dieses Typs braucht die Einfuegebarriere und ist in `#[no_gc]`
/// verboten.
pub(crate) fn ist_gc_zeiger(t: &Type) -> bool {
    ist_gc_ptr(t) || ist_gc_weak(t)
}

/// `Gc[T]` — starker, erstklassiger Zeiger.
pub(crate) fn ist_gc_ptr(t: &Type) -> bool {
    match t {
        Type::Ptr { inner, .. } => match **inner {
            Type::Struct(i) => klasse_von_struct(i).is_some(),
            _ => false,
        },
        _ => false,
    }
}

/// `GcWeak[T]` — schwacher Verweis (Zwei-Wort-Struct).
pub(crate) fn ist_gc_weak(t: &Type) -> bool {
    match t {
        Type::Struct(i) => REG.with(|r| r.borrow().klassen.iter().any(|k| k.weak_idx == *i)),
        _ => false,
    }
}

fn klasse_von_struct(idx: usize) -> Option<usize> {
    REG.with(|r| r.borrow().klassen.iter().position(|k| k.struct_idx == idx))
}

// ------------------------------------------------------------- Parser-Hooks

impl<'a> Parser<'a> {
    /// `gc class Name [extends Basis] { feld: Typ, … }`
    fn gc_class_decl(&mut self) {
        let start = self.bump(); // 'gc'
        self.bump(); // 'class'
        let (name, nspan) = match self.ident("nach 'gc class'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        let mut basis: Option<(String, Span)> = None;
        if matches!(self.kind(), TokKind::Ident(n) if n == "extends") {
            self.bump();
            match self.ident("nach 'extends'") {
                Some((b, bsp)) => basis = Some((b, bsp)),
                None => {
                    self.recovering = false;
                    self.sync_item();
                    return;
                }
            }
            if self.at(&TokKind::Comma) {
                let sp = self.span();
                self.dg.error_note(
                    sp,
                    "mehrfachvererbung ist nicht erlaubt".to_string(),
                    "SPEC 4.4: 'gc class' hat hoechstens EINE basis",
                );
                self.recovering = false;
                self.sync_item();
                return;
            }
        }
        if !self.expect(TokKind::LBrace, "nach dem namen der gc-klasse") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let mut felder: Vec<Feld> = Vec::new();
        loop {
            while self.eat(&TokKind::Comma) {}
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (fname, fspan) = match self.ident("fuer ein feld der gc-klasse") {
                Some(x) => x,
                None => break,
            };
            if !self.expect(TokKind::Colon, "nach dem feldnamen") {
                break;
            }
            let ty = match self.parse_type() {
                Some(t) => t,
                None => break,
            };
            if felder.iter().any(|f| f.name == fname) {
                self.dg.error(
                    fspan,
                    format!("feld '{}' ist in 'gc class {}' bereits deklariert", fname, name),
                );
            } else {
                felder.push(Feld { name: fname, ty, span: fspan });
            }
            if self.recovering {
                break;
            }
            if !self.eat(&TokKind::Comma) {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.close(TokKind::RBrace, "am ende der gc-klasse");
        self.recovering = false;
        let span = Parser::join(start, end);
        let _ = span;
        if index_von(&name).is_some() {
            self.dg
                .error(nspan, format!("'gc class {}' ist bereits deklariert", name));
            return;
        }
        REG.with(|r| {
            let mut reg = r.borrow_mut();
            let tid = reg.klassen.len() as u64 + 1;
            reg.klassen.push(Klasse {
                name: name.clone(),
                span: nspan,
                basis,
                felder,
                struct_idx: usize::MAX,
                weak_idx: usize::MAX,
                tid,
                size: 0,
                stark_offs: Vec::new(),
                weak_offs: Vec::new(),
                basis_tid: 0,
                union_idx: usize::MAX,
                fertig: false,
            });
        });
    }

    /// Runde 53: `[E]` bzw. `[K, V]` nach `GcVec`/`GcMap`. Die Argumente
    /// sind VOLLE Typen (`GcVec[Gc[Node]]`), werden geprueft und dann
    /// verworfen — der Behaelter ist nominal einer. Liefert die Spanne der
    /// schliessenden Klammer.
    fn gc_sammlung_args(&mut self, name: &str, n: usize) -> Option<Span> {
        if !self.expect(TokKind::LBracket, "nach 'GcVec'/'GcMap'") {
            return None;
        }
        let mut i = 0;
        loop {
            self.parse_type()?;
            i += 1;
            if self.at(&TokKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        if i != n {
            self.dg.error_note(
                self.span(),
                format!("'{}' erwartet {} typargument(e), bekommen {}", name, n, i),
                "GcVec[E] hat eines, GcMap[K, V] hat zwei (SPEC 3.5.2)",
            );
            self.recovering = true;
            return None;
        }
        let end = self.span();
        if !self.expect(TokKind::RBracket, "nach den typargumenten") {
            return None;
        }
        Some(end)
    }

    /// `[Name]` nach `Gc`/`GcWeak`/`gc_null`/`weak_null`/`as?`.
    fn gc_typ_arg(&mut self, was: &str) -> Option<(String, Span)> {
        if !self.expect(TokKind::LBracket, was) {
            return None;
        }
        let r = self.ident(was)?;
        if !self.expect(TokKind::RBracket, "nach dem typargument") {
            return None;
        }
        Some(r)
    }
}

/// `// HOOK gc` in `parser.rs::program` — `gc class`-Deklaration.
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    let ist_gc = matches!(p.kind(), TokKind::Ident(n) if n == "gc");
    if !ist_gc {
        return false;
    }
    let ist_class = matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::Ident(n)) if n == "class");
    if !ist_class {
        return false;
    }
    p.gc_class_decl();
    true
}

/// `// HOOK gc` in `parser.rs::parse_type_inner` — `Gc[C]` und `GcWeak[C]`.
/// `name` ist bereits verbraucht, `sp` seine Position.
pub(crate) fn hook_type(p: &mut Parser, name: &str, sp: Span) -> Option<TypeExpr> {
    // Runde 53: `GcVec[E]` und `GcMap[K,V]` (SPEC §3.5.2). Beide sind
    // ZEIGER auf die Laufzeitklassen `GcVec`/`GcMap` aus lib/gc/gcvec.fi
    // bzw. lib/gc/gcmap.fi — `GcVec[Gc[Node]]` ist also genau `Gc[GcVec]`,
    // nur so geschrieben, wie es in der SPEC steht.
    //
    // WAS DAS NICHT IST, und das gehoert hierher: eine je Elementtyp EIGENE
    // Klasse. Stufe 0 hat keine generischen `gc class` — der Behaelter ist
    // nominal EINER, und der Elementtyp wird am ZUGRIFF geprueft
    // (`gcvec_anhaengen[Node](…)`), nicht am Feld. Die Typargumente werden
    // hier vollstaendig geparst (auch `Gc[Node]`), damit ein Tippfehler
    // auffaellt, danach aber verworfen. `docs/RUNDE53.md` §6 nennt den Preis.
    if name == "GcVec" || name == "GcMap" {
        if !p.at(&TokKind::LBracket) {
            return None;
        }
        let n = if name == "GcVec" { 1 } else { 2 };
        let ksp = p.gc_sammlung_args(name, n)?;
        return Some(TypeExpr::Named(
            format!("{}{}", P_TYP, name),
            Parser::join(sp, ksp),
        ));
    }
    let praefix = match name {
        "Gc" => P_TYP,
        "GcWeak" => P_WTYP,
        _ => return None,
    };
    if !p.at(&TokKind::LBracket) {
        return None;
    }
    let (klasse, ksp) = p.gc_typ_arg("nach 'Gc'/'GcWeak'")?;
    Some(TypeExpr::Named(format!("{}{}", praefix, klasse), Parser::join(sp, ksp)))
}

/// `// HOOK gc` in `parser.rs::primary` — `gc C{…}`, `gc_null[C]()`,
/// `weak_null[C]()`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    let name = match p.kind().clone() {
        TokKind::Ident(n) => n,
        _ => return None,
    };
    match name.as_str() {
        "gc" => {
            // `gc C{ … }` — Allokation auf dem GC-Heap.
            let klasse = match p.toks.get(p.pos + 1).map(|t| t.kind.clone()) {
                Some(TokKind::Ident(k)) if k != "class" => k,
                _ => return None,
            };
            if !matches!(p.toks.get(p.pos + 2).map(|t| &t.kind), Some(TokKind::LBrace)) {
                return None;
            }
            let sp = p.bump(); // 'gc'
            let ksp = p.bump(); // Klassenname
            let span = Parser::join(sp, ksp);
            let saved = p.no_struct_lit;
            p.no_struct_lit = false;
            let lit = p.struct_lit(format!("{}{}", P_NEU, klasse), span);
            p.no_struct_lit = saved;
            // Als AUFRUF verpackt: nur so sieht die `#[no_gc]`-Pruefung
            // (nogc.rs, Regel 1) die Allokation — sie prueft Aufrufnamen.
            let voll = Parser::join(span, lit.span);
            Some(p.mk(voll, ExprKind::Call(format!("{}{}", P_NEU, klasse), vec![lit], span)))
        }
        "gc_null" | "weak_null" => {
            if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::LBracket)) {
                return None;
            }
            let sp = p.bump();
            let (klasse, ksp) = p.gc_typ_arg("nach 'gc_null'/'weak_null'")?;
            let span = Parser::join(sp, ksp);
            if !p.expect(TokKind::LParen, "nach dem typargument") {
                return None;
            }
            if !p.expect(TokKind::RParen, "nach '(' — der nullwert hat kein argument") {
                return None;
            }
            if name == "gc_null" {
                // `0 as Gc[C]` — der Nullwert ist der Nullzeiger.
                let null = p.mk(span, ExprKind::Int(0));
                Some(p.mk(
                    span,
                    ExprKind::Cast(
                        Box::new(null),
                        TypeExpr::Named(format!("{}{}", P_TYP, klasse), span),
                    ),
                ))
            } else {
                // `GcWeak[C]{ __p: 0, __s: 0 }` — der leere schwache Verweis.
                let null1 = p.mk(span, ExprKind::Int(0));
                let null2 = p.mk(span, ExprKind::Int(0));
                Some(p.mk(
                    span,
                    ExprKind::StructLit(
                        weak_struct_name(&klasse),
                        vec![
                            ("__p".to_string(), null1, span),
                            ("__s".to_string(), null2, span),
                        ],
                        span,
                    ),
                ))
            }
        }
        _ => None,
    }
}

/// `// HOOK gc` in `parser.rs::postfix` — `x.as?[C]`.
/// Steht direkt nach dem verbrauchten '.'.
pub(crate) fn hook_postfix(p: &mut Parser, base: &Expr) -> Option<Expr> {
    if !matches!(p.kind(), TokKind::KwAs) {
        return None;
    }
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::Question)) {
        return None;
    }
    let sp = p.bump(); // 'as'
    p.bump(); // '?'
    let (klasse, ksp) = p.gc_typ_arg("nach '.as?'")?;
    let span = Parser::join(base.span, ksp);
    Some(p.mk(
        span,
        ExprKind::Call(format!("{}{}", P_AS, klasse), vec![base.clone()], Parser::join(sp, ksp)),
    ))
}

fn weak_struct_name(klasse: &str) -> String {
    format!("GcWeak[{}]", klasse)
}

// ---------------------------------------------------- Anmeldung im Typkontext

/// `// HOOK gc` in `sema::Checker::run` (vor `collect_structs`): meldet jede
/// `gc class` als Struct mit Praefixlayout an und berechnet Typkennung,
/// Ahnenkette und die Offsets fuer die praezise Heap-Verfolgung.
pub(crate) fn declare_classes(ck: &mut Checker) {
    let n = REG.with(|r| r.borrow().klassen.len());
    if n == 0 {
        return;
    }
    // 1. Structs anmelden (Layout kommt in Schritt 3).
    for i in 0..n {
        let (name, span) =
            match REG.with(|r| r.borrow().klassen.get(i).map(|k| (k.name.clone(), k.span))) {
                Some(x) => x,
                None => continue,
            };
        if ck.tcx.lookup(&name).is_some() {
            ck.dg
                .error(span, format!("typ '{}' ist bereits deklariert", name));
        }
        let sidx = ck.tcx.declare(&format!("gc {}", name));
        let widx = ck.tcx.declare(&weak_struct_name(&name));
        ck.tcx
            .set_fields(widx, vec![("__p".to_string(), Type::U64), ("__s".to_string(), Type::U64)]);
        REG.with(|r| {
            let mut reg = r.borrow_mut();
            if let Some(k) = reg.klassen.get_mut(i) {
                k.struct_idx = sidx;
                k.weak_idx = widx;
            }
        });
    }
    // 2. Basis pruefen (Existenz, keine Kreise).
    for i in 0..n {
        let (name, basis) =
            match REG.with(|r| r.borrow().klassen.get(i).map(|k| (k.name.clone(), k.basis.clone())))
            {
                Some(x) => x,
                None => continue,
            };
        let (bname, bspan) = match basis {
            Some(b) => b,
            None => continue,
        };
        let bi = match index_von(&bname) {
            Some(b) => b,
            None => {
                ck.dg.error_note(
                    bspan,
                    format!("unbekannte basisklasse '{}'", bname),
                    "eine basis muss selbst mit 'gc class' deklariert sein (SPEC 4.4)",
                );
                REG.with(|r| {
                    if let Some(k) = r.borrow_mut().klassen.get_mut(i) {
                        k.basis = None;
                    }
                });
                continue;
            }
        };
        if kreis(bi, i) {
            ck.dg.error(
                bspan,
                format!("die vererbungskette von 'gc class {}' ist ringfoermig", name),
            );
            REG.with(|r| {
                if let Some(k) = r.borrow_mut().klassen.get_mut(i) {
                    k.basis = None;
                }
            });
        }
    }
}

/// `// HOOK gc` in `sema::add_items_inner` (NACH `collect_structs`): legt das
/// Feldlayout jeder Klasse fest. Erst hier sind die Structs des Programms
/// bekannt — ein Structfeld in einer gc-Klasse bekommt so die richtige
/// Meldung statt „unbekannter typ".
pub(crate) fn layout_classes(ck: &mut Checker) {
    let n = REG.with(|r| r.borrow().klassen.len());
    // Layout in topologischer Reihenfolge (Basis zuerst).
    for _ in 0..n {
        let mut fortschritt = false;
        for i in 0..n {
            let (fertig, basis) = REG.with(|r| {
                let reg = r.borrow();
                match reg.klassen.get(i) {
                    Some(k) => (k.fertig, k.basis.clone()),
                    None => (true, None),
                }
            });
            if fertig {
                continue;
            }
            if let Some((b, _)) = &basis {
                let bfertig = index_von(b)
                    .and_then(|bi| REG.with(|r| r.borrow().klassen.get(bi).map(|k| k.fertig)))
                    .unwrap_or(true);
                if !bfertig {
                    continue;
                }
            }
            lege_aus(ck, i);
            fortschritt = true;
        }
        if !fortschritt {
            break;
        }
    }
}

/// Erreicht `von` ueber die Basiskette `ziel`?
fn kreis(von: usize, ziel: usize) -> bool {
    let mut cur = von;
    for _ in 0..1024 {
        if cur == ziel {
            return true;
        }
        let b = REG.with(|r| r.borrow().klassen.get(cur).and_then(|k| k.basis.clone()));
        match b.and_then(|(n, _)| index_von(&n)) {
            Some(next) => cur = next,
            None => return false,
        }
    }
    true
}

fn lege_aus(ck: &mut Checker, i: usize) {
    let k = match REG.with(|r| r.borrow().klassen.get(i).cloned()) {
        Some(k) => k,
        None => return,
    };
    // Basisfelder liegen VORNE (Praefixlayout, kostenlose Aufwaertsumwandlung).
    let mut felder: Vec<(String, Type)> = Vec::new();
    let mut basis_tid = 0u64;
    if let Some((bname, _)) = &k.basis {
        if let Some(bi) = index_von(bname) {
            let (bidx, btid) = REG.with(|r| {
                let reg = r.borrow();
                match reg.klassen.get(bi) {
                    Some(b) => (b.struct_idx, b.tid),
                    None => (usize::MAX, 0),
                }
            });
            basis_tid = btid;
            if let Some(bd) = ck.tcx.structs.get(bidx) {
                for f in &bd.fields {
                    felder.push((f.name.clone(), f.ty.clone()));
                }
            }
        }
    }
    for f in &k.felder {
        if felder.iter().any(|(n, _)| *n == f.name) {
            ck.dg.error_note(
                f.span,
                format!("feld '{}' ist schon in der basis von 'gc class {}' vergeben", f.name, k.name),
                "geerbte feldnamen duerfen nicht erneut vergeben werden (SPEC 4.4)",
            );
            continue;
        }
        let t = ck.resolve_ty(&f.ty);
        if !feldtyp_erlaubt(&t) {
            ck.dg.error_note(
                f.span,
                format!(
                    "feldtyp {} ist in einer gc-klasse nicht erlaubt",
                    ck.tcx.name_of(&t)
                ),
                "erlaubt sind ganzzahlen, bool, zeiger, Gc[T], GcWeak[T] und arrays davon",
            );
            continue;
        }
        felder.push((f.name.clone(), t));
    }
    let sidx = k.struct_idx;
    if sidx == usize::MAX {
        return;
    }
    ck.tcx.set_fields(sidx, felder);
    // Offsets fuer die compilergenerierte Verfolgung einsammeln.
    let mut stark = Vec::new();
    let mut weak = Vec::new();
    let mut size = 0;
    if let Some(d) = ck.tcx.structs.get(sidx) {
        size = d.size;
        for f in &d.fields {
            if ist_gc_ptr(&f.ty) {
                stark.push(f.offset);
            } else if ist_gc_weak(&f.ty) {
                weak.push(f.offset);
            }
        }
    }
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        if let Some(k) = reg.klassen.get_mut(i) {
            k.size = size;
            k.stark_offs = stark;
            k.weak_offs = weak;
            k.basis_tid = basis_tid;
            k.fertig = true;
        }
    });
}

fn feldtyp_erlaubt(t: &Type) -> bool {
    match t {
        Type::Error => true, // Fehler ist schon gemeldet
        Type::Array(e, _) => feldtyp_erlaubt(e),
        Type::Struct(_) => ist_gc_weak(t),
        Type::Void | Type::UntypedInt => false,
        Type::Ptr { .. } => true,
        _ => true,
    }
}

// -------------------------------------------------------------- Typpruefung

/// `// HOOK gc` in `sema::resolve_ty_d`: `Gc[C]`, `GcWeak[C]` und der
/// verbotene Gebrauch eines `gc class`-Namens als gewoehnlicher Typ.
pub(crate) fn hook_resolve_ty(ck: &mut Checker, te: &TypeExpr) -> Option<Type> {
    let (name, span) = match te {
        TypeExpr::Named(n, s) => (n.as_str(), *s),
        _ => return None,
    };
    if let Some(klasse) = name.strip_prefix(P_TYP) {
        return Some(match index_von(klasse) {
            Some(i) => {
                let sidx = REG.with(|r| r.borrow().klassen[i].struct_idx);
                Type::ptr(Type::Struct(sidx), true)
            }
            None => {
                unbekannte_klasse(ck, klasse, span);
                Type::Error
            }
        });
    }
    if let Some(klasse) = name.strip_prefix(P_WTYP) {
        return Some(match index_von(klasse) {
            Some(i) => Type::Struct(REG.with(|r| r.borrow().klassen[i].weak_idx)),
            None => {
                unbekannte_klasse(ck, klasse, span);
                Type::Error
            }
        });
    }
    // `let x: Node` — ein gc-class-Wert lebt NUR auf dem GC-Heap.
    if ist_klasse(name) {
        ck.dg.error_note(
            span,
            format!("'{}' ist eine gc-klasse und kann kein wert sein", name),
            "ein 'gc class'-wert lebt nur auf dem GC-Heap: schreibe 'Gc[".to_string()
                + name
                + "]' (SPEC 3.5.1)",
        );
        return Some(Type::Error);
    }
    None
}

fn unbekannte_klasse(ck: &mut Checker, name: &str, span: Span) {
    ck.dg.error_note(
        span,
        format!("unbekannte gc-klasse '{}'", name),
        "eine gc-klasse wird mit 'gc class Name { … }' deklariert",
    );
}

/// `gc C{ … }` liefert `AllocError!Gc[C]` (gerufen aus `hook_call`).
fn check_neu(
    ck: &mut Checker,
    name: &str,
    felder: &[(String, Expr, Span)],
    nspan: Span,
) -> Option<Type> {
    let klasse = name.strip_prefix(P_NEU)?;
    let i = match index_von(klasse) {
        Some(i) => i,
        None => {
            unbekannte_klasse(ck, klasse, nspan);
            for (_, e, _) in felder {
                ck.type_out_expr(e);
            }
            return Some(Type::Error);
        }
    };
    let sidx = REG.with(|r| r.borrow().klassen[i].struct_idx);
    let decl: Vec<(String, Type)> = match ck.tcx.structs.get(sidx) {
        Some(d) => d.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect(),
        None => Vec::new(),
    };
    let mut gesehen: Vec<&str> = Vec::new();
    for (fname, fexpr, fspan) in felder {
        match decl.iter().find(|(n, _)| n == fname) {
            Some((_, want)) => {
                let got = ck.expr(fexpr, Some(want));
                if !got.is_error() && !zuweisbar(&got, want) {
                    ck.dg.error(
                        *fspan,
                        format!(
                            "feld '{}' von 'gc class {}' erwartet {}, gefunden {}",
                            fname,
                            klasse,
                            ck.tcx.name_of(want),
                            ck.tcx.name_of(&got)
                        ),
                    );
                }
            }
            None => {
                ck.type_out_expr(fexpr);
                ck.dg.error(
                    *fspan,
                    format!("'gc class {}' hat kein feld '{}'", klasse, fname),
                );
            }
        }
        if gesehen.contains(&fname.as_str()) {
            ck.dg
                .error(*fspan, format!("feld '{}' ist doppelt angegeben", fname));
        }
        gesehen.push(fname);
    }
    let fehlend: Vec<String> = decl
        .iter()
        .filter(|(n, _)| !gesehen.contains(&n.as_str()))
        .map(|(n, _)| n.clone())
        .collect();
    if !fehlend.is_empty() {
        ck.dg.error_note(
            nspan,
            format!(
                "in 'gc {}{{…}}' fehlen die felder: {}",
                klasse,
                fehlend.join(", ")
            ),
            "bei einer gc-allokation muessen ALLE felder angegeben werden",
        );
    }
    let u = alloc_union(ck, Type::ptr(Type::Struct(sidx), true), nspan);
    if let Type::Struct(ui) = u {
        REG.with(|r| {
            if let Some(k) = r.borrow_mut().klassen.get_mut(i) {
                k.union_idx = ui;
            }
        });
    }
    Some(u)
}

/// `AllocError!T` — die fehlbare Allokation (DESIGNZIELE §2).
fn alloc_union(ck: &mut Checker, val: Type, span: Span) -> Type {
    match crate::errors::union_type(ck, ERR_SET, &val) {
        Some(t) => t,
        None => {
            ck.dg.error_note(
                span,
                format!("die fehlermenge '{}' ist nicht deklariert", ERR_SET),
                "sie kommt mit der GC-Laufzeit (lib/gc/gc.fi) und wird automatisch eingezogen",
            );
            Type::Error
        }
    }
}

/// `// HOOK gc` in `sema::call`: `weak(g)`, `stark(w)`, `x.as?[C]` und die
/// beiden Compilerintrinsics. Liefert `None`, wenn es nichts davon ist.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if let Some(lit) = name.strip_prefix(P_NEU) {
        let _ = lit;
        let felder: Vec<(String, Expr, Span)> = match args.first().map(|a| &a.kind) {
            Some(ExprKind::StructLit(_, f, _)) => f.clone(),
            _ => Vec::new(),
        };
        let t = check_neu(ck, name, &felder, nspan)?;
        if let Some(a) = args.first() {
            ck.record(a.id, t.clone());
        }
        return Some(t);
    }
    if name == INTR_STATE || name == INTR_REGS {
        for a in args {
            ck.type_out_expr(a);
        }
        return Some(Type::ptr(Type::U8, true));
    }
    if let Some(klasse) = name.strip_prefix(P_AS) {
        return Some(check_as(ck, klasse, args, nspan));
    }
    if (name != "weak" && name != "stark") || ck.fns.contains_key(name) {
        return None;
    }
    if args.len() != 1 {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error(
            espan,
            format!("'{}' erwartet genau ein argument, gefunden {}", name, args.len()),
        );
        return Some(Type::Error);
    }
    let at = ck.expr(&args[0], None);
    if at.is_error() {
        return Some(Type::Error);
    }
    if name == "weak" {
        let sidx = match &at {
            Type::Ptr { inner, .. } => match **inner {
                Type::Struct(i) => i,
                _ => usize::MAX,
            },
            _ => usize::MAX,
        };
        return Some(match klasse_von_struct(sidx) {
            Some(i) => Type::Struct(REG.with(|r| r.borrow().klassen[i].weak_idx)),
            None => {
                ck.dg.error_note(
                    args[0].span,
                    format!(
                        "'weak' erwartet einen Gc[T], gefunden {}",
                        ck.tcx.name_of(&at)
                    ),
                    "ein schwacher verweis entsteht nur aus einem starken",
                );
                Type::Error
            }
        });
    }
    // stark(w)
    let idx = match &at {
        Type::Struct(i) => *i,
        _ => usize::MAX,
    };
    let treffer = REG.with(|r| r.borrow().klassen.iter().position(|k| k.weak_idx == idx));
    Some(match treffer {
        Some(i) => {
            let sidx = REG.with(|r| r.borrow().klassen[i].struct_idx);
            Type::ptr(Type::Struct(sidx), true)
        }
        None => {
            ck.dg.error_note(
                args[0].span,
                format!(
                    "'stark' erwartet einen GcWeak[T], gefunden {}",
                    ck.tcx.name_of(&at)
                ),
                "'stark' wertet einen schwachen verweis auf",
            );
            Type::Error
        }
    })
}

fn check_as(ck: &mut Checker, klasse: &str, args: &[Expr], nspan: Span) -> Type {
    let ziel = match index_von(klasse) {
        Some(i) => i,
        None => {
            for a in args {
                ck.type_out_expr(a);
            }
            unbekannte_klasse(ck, klasse, nspan);
            return Type::Error;
        }
    };
    let arg = match args.first() {
        Some(a) => a,
        None => return Type::Error,
    };
    let at = ck.expr(arg, None);
    if at.is_error() {
        return Type::Error;
    }
    let quelle = match &at {
        Type::Ptr { inner, .. } => match **inner {
            Type::Struct(i) => klasse_von_struct(i),
            _ => None,
        },
        _ => None,
    };
    let quelle = match quelle {
        Some(q) => q,
        None => {
            ck.dg.error(
                arg.span,
                format!(
                    "'.as?[{}]' erwartet einen Gc[T], gefunden {}",
                    klasse,
                    ck.tcx.name_of(&at)
                ),
            );
            return Type::Error;
        }
    };
    // Abwaerts geprueft: das Ziel muss in der Ahnenkette die Quelle haben.
    if quelle != ziel && !ist_nachfahre(ziel, quelle) && !ist_nachfahre(quelle, ziel) {
        let (qn, zn) = REG.with(|r| {
            let reg = r.borrow();
            (reg.klassen[quelle].name.clone(), reg.klassen[ziel].name.clone())
        });
        ck.dg.error_note(
            nspan,
            format!("'{}' und '{}' sind nicht verwandt", qn, zn),
            "'.as?[T]' prueft nur innerhalb einer vererbungskette (SPEC 4.4)",
        );
        return Type::Error;
    }
    let sidx = REG.with(|r| r.borrow().klassen[ziel].struct_idx);
    Type::ptr(Type::Struct(sidx), true)
}

/// Ist `a` ein Nachfahre von `b` (echte oder gleiche Klasse ausgenommen)?
fn ist_nachfahre(a: usize, b: usize) -> bool {
    let mut cur = a;
    for _ in 0..1024 {
        let basis = REG.with(|r| r.borrow().klassen.get(cur).and_then(|k| k.basis.clone()));
        match basis.and_then(|(n, _)| index_von(&n)) {
            Some(next) => {
                if next == b {
                    return true;
                }
                cur = next;
            }
            None => return false,
        }
    }
    false
}

fn zuweisbar(got: &Type, want: &Type) -> bool {
    if got == want {
        return true;
    }
    ist_aufwaerts(got, want)
}

/// Kostenlose Aufwaertsumwandlung `Gc[Element]` -> `Gc[Node]` (SPEC §4.4).
pub(crate) fn ist_aufwaerts(got: &Type, want: &Type) -> bool {
    let (g, w) = match (got, want) {
        (Type::Ptr { inner: a, .. }, Type::Ptr { inner: b, .. }) => (a, b),
        _ => return false,
    };
    let (gi, wi) = match (&**g, &**w) {
        (Type::Struct(a), Type::Struct(b)) => (*a, *b),
        _ => return false,
    };
    match (klasse_von_struct(gi), klasse_von_struct(wi)) {
        (Some(a), Some(b)) => a == b || ist_nachfahre(a, b),
        _ => false,
    }
}

/// Sind zwei Gc-Zeiger verwandt (fuer den Identitaetsvergleich `g == h`)?
pub(crate) fn ist_verwandt(a: &Type, b: &Type) -> bool {
    ist_aufwaerts(a, b) || ist_aufwaerts(b, a)
}

/// `// HOOK gc` in `sema::probe_d`: Typ von `weak(g)`, `stark(w)` und
/// `x.as?[C]` OHNE Pruefung — damit ein Ganzzahlliteral daneben seinen Typ
/// bekommt (`if g.feld != 5`).
pub(crate) fn probe_typ(name: &str, arg: Option<&Type>) -> Option<Type> {
    if let Some(klasse) = name.strip_prefix(P_AS) {
        let i = index_von(klasse)?;
        return Some(Type::ptr(Type::Struct(REG.with(|r| r.borrow().klassen[i].struct_idx)), true));
    }
    let at = arg?;
    match name {
        "weak" => {
            let i = match at {
                Type::Ptr { inner, .. } => match **inner {
                    Type::Struct(s) => klasse_von_struct(s)?,
                    _ => return None,
                },
                _ => return None,
            };
            Some(Type::Struct(REG.with(|r| r.borrow().klassen[i].weak_idx)))
        }
        "stark" => {
            let idx = match at {
                Type::Struct(i) => *i,
                _ => return None,
            };
            let i = REG.with(|r| r.borrow().klassen.iter().position(|k| k.weak_idx == idx))?;
            Some(Type::ptr(Type::Struct(REG.with(|r| r.borrow().klassen[i].struct_idx)), true))
        }
        _ => None,
    }
}

/// `// HOOK gc` in `sema::field_type`: Feldzugriff durch `Gc[T]` hindurch.
/// Liefert den Struct-Index, wenn `base` ein Gc-Zeiger ist.
pub(crate) fn hook_field_base(base: &Type) -> Option<usize> {
    match base {
        Type::Ptr { inner, .. } => match **inner {
            Type::Struct(i) if klasse_von_struct(i).is_some() => Some(i),
            _ => None,
        },
        _ => None,
    }
}

// ------------------------------------------------------- Angaben fuer Lowering

/// Typkennung, Groesse und Struct-Index einer Klasse (fuer `gc_lower.rs`).
pub(crate) fn klasse_info(name: &str) -> Option<(u64, u64, usize)> {
    let i = index_von(name)?;
    REG.with(|r| {
        let reg = r.borrow();
        reg.klassen.get(i).map(|k| (k.tid, k.size, k.struct_idx))
    })
}

/// Struct-Index der Fehlerunion `AllocError!Gc[C]` (fuer `gc_lower.rs`).
pub(crate) fn union_idx(name: &str) -> Option<usize> {
    let i = index_von(name)?;
    let u = REG.with(|r| r.borrow().klassen.get(i).map(|k| k.union_idx))?;
    if u == usize::MAX {
        None
    } else {
        Some(u)
    }
}

/// Name der Klasse hinter einem `__gc#neu:`/`__gc#as:`-Aufruf.
pub(crate) fn klasse_aus_neu(name: &str) -> Option<&str> {
    name.strip_prefix(P_NEU)
}

pub(crate) fn klasse_aus_as(name: &str) -> Option<&str> {
    name.strip_prefix(P_AS)
}

// ------------------------------------------------------------- Codegenerator

/// Offset des Registerrettungsbereichs im Zustandsblock (Bytes).
pub(crate) const REG_SAVE_OFF: u64 = 3968;
/// Groesse des Zustandsblocks (Bytes).
pub(crate) const STATE_SIZE: u64 = 4096;
/// Label des Zustandsblocks (`.data`, dateilokal).
pub(crate) const STATE_LABEL: &str = ".L__gc_zustand";
/// Label der Typtabelle (`.rodata`, dateilokal).
pub(crate) const TABLE_LABEL: &str = ".L__gc_typtabelle";

/// Die compilergenerierte Typtabelle: aus dem Feldlayout, je Typ ein Eintrag
/// von 8 Woertern (SPEC §3.5.3 — praezise Heap-Verfolgung).
///
/// ```text
/// wort 0: n_typen
/// eintrag(tid) = tabelle + 8 + (tid-1)*64
///   +0  groesse in bytes
///   +8  typkennung der basis (0 = keine)
///   +16 anzahl starker felder
///   +24 adresse der offsetliste (Gc[T])
///   +32 anzahl schwacher felder
///   +40 adresse der offsetliste (GcWeak[T])
///   +48 typkennung (zur probe)
///   +56 reserviert
/// ```
pub(crate) fn typtabelle_asm() -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    REG.with(|r| {
        let reg = r.borrow();
        let _ = writeln!(out, ".section .rodata");
        let _ = writeln!(out, ".align 8");
        let _ = writeln!(out, "{}:", TABLE_LABEL);
        let _ = writeln!(out, "    .quad {}", reg.klassen.len());
        for k in &reg.klassen {
            let _ = writeln!(out, "    .quad {}", k.size);
            let _ = writeln!(out, "    .quad {}", k.basis_tid);
            let _ = writeln!(out, "    .quad {}", k.stark_offs.len());
            let _ = writeln!(out, "    .quad {}.s{}", TABLE_LABEL, k.tid);
            let _ = writeln!(out, "    .quad {}", k.weak_offs.len());
            let _ = writeln!(out, "    .quad {}.w{}", TABLE_LABEL, k.tid);
            let _ = writeln!(out, "    .quad {}", k.tid);
            let _ = writeln!(out, "    .quad 0");
        }
        for k in &reg.klassen {
            let _ = writeln!(out, "{}.s{}:", TABLE_LABEL, k.tid);
            for o in &k.stark_offs {
                let _ = writeln!(out, "    .quad {}", o);
            }
            let _ = writeln!(out, "    .quad 0");
            let _ = writeln!(out, "{}.w{}:", TABLE_LABEL, k.tid);
            for o in &k.weak_offs {
                let _ = writeln!(out, "    .quad {}", o);
            }
            let _ = writeln!(out, "    .quad 0");
        }
        // Zustandsblock: beschreibbar, wort 0 zeigt auf die Typtabelle.
        let _ = writeln!(out, ".section .data");
        let _ = writeln!(out, ".align 16");
        let _ = writeln!(out, "{}:", STATE_LABEL);
        let _ = writeln!(out, "    .quad {}", TABLE_LABEL);
        let _ = writeln!(out, "    .zero {}", STATE_SIZE - 8);
        let _ = writeln!(out, ".section .text");
    });
    out
}

// ------------------------------------------------------------------ Laufzeit

/// Die Sammler-Laufzeit als lesbares Firn (`lib/gc/gc.fi`), eingebettet.
const LAUFZEIT: &str = include_str!("../../lib/gc/gc.fi");

/// **Runde 53** — die Sammlungen mit veraenderlicher Laenge (`SPEC` §3.5.2).
///
/// Sie stehen in EIGENEN Dateien und werden an `gc.fi` angehaengt: zusammen
/// sind sie ein Modul, also sehen sie dessen Namen (`KOPF`, `F_SLOTS`,
/// `__gc_alloc_raw`, `__gc_barrier`) ohne `import`. Angehaengt wird nur,
/// wenn das Programm sie wirklich braucht — ein Programm mit `gc class`,
/// aber ohne Sammlungen, erzeugt danach denselben Code wie vorher.
const LAUFZEIT_VEC: &str = include_str!("../../lib/gc/gcvec.fi");
const LAUFZEIT_MAP: &str = include_str!("../../lib/gc/gcmap.fi");

/// Pfadname der eingezogenen Laufzeit in Fehlermeldungen und `.debug_line`.
pub(crate) const LAUFZEIT_PFAD: &str = "lib/gc/gc.fi";

/// Braucht dieses Programm die Laufzeit? Entschieden am Tokenstrom: irgendwo
/// stehen die beiden Bezeichner `gc class` nebeneinander.
pub(crate) fn quelle_braucht_gc(toks: &[crate::lexer::Token]) -> bool {
    toks.windows(2).any(|w| {
        matches!(&w[0].kind, TokKind::Ident(a) if a == "gc")
            && matches!(&w[1].kind, TokKind::Ident(b) if b == "class")
    })
}

/// Deklariert die Quelle schon selbst `error AllocError { … }`?
pub(crate) fn quelle_hat_allocerror(toks: &[crate::lexer::Token]) -> bool {
    toks.windows(2).any(|w| {
        matches!(&w[0].kind, TokKind::KwError)
            && matches!(&w[1].kind, TokKind::Ident(b) if b == ERR_SET)
    })
}

/// **Runde 53** — braucht dieses Programm die Sammlungen (`GcVec`/`GcMap`)?
///
/// Entschieden am Tokenstrom wie `quelle_braucht_gc`: irgendwo steht ein
/// Bezeichner, der mit `GcVec`, `GcMap`, `gcvec_` oder `gcmap_` beginnt.
/// Der Praefixtest statt eines Gleichheitstests deckt beides ab — den Typ
/// `GcVec[Gc[T]]` und den Aufruf `gcvec_anhaengen[T](…)`.
pub(crate) fn quelle_braucht_sammlungen(toks: &[crate::lexer::Token]) -> bool {
    toks.iter().any(|t| match &t.kind {
        TokKind::Ident(n) => {
            n.starts_with("GcVec")
                || n.starts_with("GcMap")
                || n.starts_with("gcvec_")
                || n.starts_with("gcmap_")
        }
        _ => false,
    })
}

/// Deklariert die Wurzeldatei selbst `fn __gc_finalisiere`?
pub(crate) fn quelle_hat_finalisierer(toks: &[crate::lexer::Token]) -> bool {
    toks.windows(2).any(|w| {
        matches!(&w[0].kind, TokKind::KwFn)
            && matches!(&w[1].kind, TokKind::Ident(b) if b == FN_FINAL)
    })
}

/// Die leere Voreinstellung des Finalisierer-Verteilers.
fn finalisierer_default() -> String {
    let mut s = String::new();
    s.push_str("// Runde 47: Voreinstellung des Finalisierer-Verteilers. Das Programm\n");
    s.push_str("// deklariert keinen eigenen, also tut das Aufraeumen nichts.\n");
    s.push_str("fn ");
    s.push_str(FN_FINAL);
    s.push_str("(art: u64, p: *mut u8) {\n");
    s.push_str("    let _unbenutzt: u64 = art + (p as u64)\n");
    s.push_str("}\n");
    s
}

/// Quelltext der Laufzeit. `mit_fehlermenge = false`, wenn das Programm
/// `AllocError` bereits selbst deklariert (Fehlermengennamen sind programmweit).
/// `mit_finalisierer = false`, wenn die Wurzeldatei den Verteiler selbst
/// mitbringt.
pub(crate) fn laufzeit_quelle(
    mit_fehlermenge: bool,
    mit_finalisierer: bool,
    mit_sammlungen: bool,
) -> String {
    let mut s = String::new();
    if mit_fehlermenge {
        s.push_str("error AllocError { OutOfMemory }\n");
    } else {
        s.push_str("// AllocError wird vom Programm selbst deklariert\n");
    }
    if mit_finalisierer {
        s.push_str(&finalisierer_default());
    } else {
        s.push_str("// __gc_finalisiere wird vom Programm selbst deklariert\n");
    }
    s.push_str(LAUFZEIT);
    if mit_sammlungen {
        s.push_str(LAUFZEIT_VEC);
        s.push_str(LAUFZEIT_MAP);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interne_namen_sind_gc_allokationen() {
        assert!(ist_gc_alloc_aufruf("gc Node"));
        assert!(!ist_gc_alloc_aufruf("gcNode"));
        assert!(ist_gc_alloc_aufruf("gc_collect"));
        assert!(ist_gc_alloc_aufruf("dom__gc_collect"));
        assert!(!ist_gc_alloc_aufruf("gc_collectx"));
        assert!(!ist_gc_alloc_aufruf("tokenize"));
    }

    #[test]
    fn ohne_klassen_ist_kein_typ_ein_gc_zeiger() {
        hook_reset();
        assert!(!ist_gc_zeiger(&Type::ptr(Type::U8, true)));
        assert!(!ist_gc_zeiger(&Type::U64));
        assert!(!hat_klassen());
    }

    #[test]
    fn laufzeit_enthaelt_die_pflichtnamen() {
        let q = laufzeit_quelle(true, true, true);
        for n in ["gc_init", "gc_collect", "gc_live_objects", FN_ALLOC, FN_WEAK, FN_STARK, FN_AS] {
            assert!(q.contains(n), "laufzeit ohne '{}'", n);
        }
        assert!(q.contains("error AllocError"));
        assert!(!laufzeit_quelle(false, true, false).contains("error AllocError {"));
        // Runde 47: der Verteiler ist genau EINMAL da — entweder als
        // Voreinstellung oder aus dem Programm, nie doppelt.
        assert!(q.contains("fn __gc_finalisiere(art: u64, p: *mut u8) {"));
        assert!(!laufzeit_quelle(true, false, false).contains("fn __gc_finalisiere(art: u64, p: *mut u8) {"));
        assert!(q.contains("gc_finalisierer_setzen"));
        // Runde 53: die Sammlungen kommen nur dazu, wenn sie gebraucht werden.
        for n in ["gcvec_anhaengen", "gcmap_setzen", "gc class GcSlots"] {
            assert!(q.contains(n), "laufzeit ohne '{}'", n);
            assert!(
                !laufzeit_quelle(true, true, false).contains(n),
                "'{}' auch ohne Sammlungen dabei",
                n
            );
        }
        assert!(q.contains("gc_wurzel_anmelden"));
    }
}
