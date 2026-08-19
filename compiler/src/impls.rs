//! `impl Typ { fn methode(*mut self, …) }` — Methoden auf struct-Typen
//! (Runde 45).
//!
//! ## Was hier passiert — und was ausdruecklich nicht
//!
//! Eine Methode ist in Firn **keine neue Art von Ding**. `impl` ist eine
//! Schreibhilfe mit genau zwei Wirkungen:
//!
//!  1. `impl T { fn m(*mut self, x: i32) … }` legt die gewoehnliche Funktion
//!     `T__m(self: *mut T, x: i32)` an — mehr nicht. Sie steht danach in
//!     `Program::funcs` wie jede andere und geht durch dieselbe Typpruefung,
//!     dasselbe Lowering, denselben Codegenerator.
//!  2. `x.m(a)` wird zum Aufruf dieser Funktion. Welche Funktion gemeint ist,
//!     entscheidet **allein der statische Typ des Empfaengers** — keine
//!     vtable, kein dynamischer Versand, kein Suchen zur Laufzeit. Steht der
//!     Typ fest, steht der Sprungbefehl fest.
//!
//! Der Aufruf traegt bis zur Typpruefung den Namen `"methode m"`. Das
//! Leerzeichen macht ihn zu einem Namen, der aus keinem Bezeichner des
//! Quelltextes entstehen kann — dieselbe Bauart wie `"gc C"` in `gc.rs`.
//! `sema.rs` loest ihn auf, `lower.rs` leitet dieselbe Aufloesung noch einmal
//! ab. Beide rechnen aus demselben Material (Empfaengertyp + Methodenname);
//! eine Seitentabelle zwischen den Phasen gibt es bewusst nicht, denn sie
//! muesste in `firnc1` mitgeschleppt werden, ohne etwas zu koennen, was der
//! Typ nicht schon sagt.
//!
//! ## Der Empfaenger — warum `*self` und nicht `&self`
//!
//! Firn hat **keine Referenzen**. Es hat Zeiger (`*T`, `*mut T`) und den
//! Adressoperator `&x`. Ein `&self` waere ein neuer Begriff, der in der
//! ganzen uebrigen Sprache nicht vorkommt. Deshalb wird der Empfaenger so
//! geschrieben, wie ein Parameter in Firn geschrieben wird:
//!
//! ```text
//! impl Bytes {
//!     fn laenge(*self) -> usize        // self: *Bytes
//!     fn dazu(*mut self, v: u8)        // self: *mut Bytes
//!     fn kopf(self) -> u8              // self: Bytes   (Kopie)
//! }
//! ```
//!
//! Am Aufrufort wird **eine** Anpassung gemacht, und zwar die, die man sonst
//! von Hand schreibt: verlangt die Methode einen Zeiger und liegt der
//! Empfaenger als Wert vor, nimmt der Compiler seine Adresse (`&x`). Liegt er
//! schon als Zeiger vor, wird er durchgereicht. Mehr Automatik gibt es nicht:
//! kein automatisches Dereferenzieren, keine Kette von `*`, keine Umwege ueber
//! Felder. Wer eine Kopie will, schreibt `(*p).m()`.
//!
//! Dass `*self` und `*mut self` in der Typpruefung **denselben** Empfaenger
//! zulassen, ist keine Nachlaessigkeit dieser Datei, sondern die Regel der
//! Sprache: `sema::compatible` vergleicht Zeiger ohne die Veraenderlichkeit
//! (`*T` und `*mut T` sind fuereinander einsetzbar). `*mut self` sagt also
//! genau das, was `*mut T` als Parametertyp heute sagt — Absicht, nicht
//! Zwang. Aendert sich diese Regel einmal fuer Parameter, aendert sie sich
//! fuer den Empfaenger von selbst mit.
//!
//! ## Namensaufloesung (SPEC-Ergaenzung §12.6)
//!
//! * Methoden und freie Funktionen liegen in **einem** Namensraum, aber unter
//!   verschiedenen Namen: die Methode `m` von `T` heisst `T__m`. `m(x)` findet
//!   deshalb nie eine Methode, und `x.m()` findet nie eine freie Funktion.
//!   Verdecken ist damit ausgeschlossen — es gibt nichts zu verdecken.
//! * Im Modul `str` wird aus `T__m` beim Zusammenfuehren `str__T__m`
//!   (`modules.rs`), und aus dem Typ `T` wird `str__T`. Die Aufloesung
//!   `Strukturname ++ "__" ++ Methode` stimmt danach weiter — sie rechnet
//!   immer mit dem Namen, den der Typ zu diesem Zeitpunkt traegt.
//! * Generische Typen (`Vec[T]`) haben in dieser Runde keine Methoden; siehe
//!   `docs/RUNDE45.md`, Abschnitt „Bewusst weggelassen".

use crate::ast::{Expr, ExprKind, FnDecl, Param, Program, TypeExpr, UnOp};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use std::collections::HashMap;

use crate::sema::{Checker, FnSig, TypeInfo};
use crate::types::{Type, TypeCtx};

/// Praefix des noch nicht aufgeloesten Methodenaufrufs im AST.
/// Das Leerzeichen macht ihn unerreichbar fuer den Quelltext.
pub(crate) const P_RUF: &str = "methode ";
/// Trenner im Namen der Methodenfunktion: `Typ__methode`.
pub(crate) const TRENNER: &str = "__";

/// Ist das ein noch nicht aufgeloester Methodenaufruf? Liefert den
/// Methodennamen.
pub(crate) fn methodenname(name: &str) -> Option<&str> {
    name.strip_prefix(P_RUF)
}

/// Name der Funktion hinter `Typ.methode`.
pub(crate) fn fn_name(typ: &str, methode: &str) -> String {
    format!("{}{}{}", typ, TRENNER, methode)
}

// ------------------------------------------------------------------- Parser

/// `// HOOK impl` in `parser.rs::program` — `impl Typ { … }` und (Runde 46)
/// `impl Schnittstelle for Typ { … }`.
///
/// `impl` ist KEIN Schluesselwort (der Tokenisierer kennt es nicht), sondern
/// ein Bezeichner in einer Stellung, in der sonst nichts stehen darf —
/// dieselbe Loesung wie `gc class` (gc.rs). Damit bleibt `impl` als
/// Variablenname gueltig. `for` ist dagegen schon ein Schluesselwort
/// (`for i in a..b`) und deshalb hier eindeutig.
pub(crate) fn hook_item(p: &mut Parser, prog: &mut Program) -> bool {
    if !matches!(p.kind(), TokKind::Ident(n) if n == "impl") {
        return false;
    }
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::Ident(_))) {
        return false;
    }
    match p.toks.get(p.pos + 2).map(|t| &t.kind) {
        Some(TokKind::LBrace) => {
            impl_decl(p, prog, false);
            true
        }
        Some(TokKind::KwFor) => {
            if !matches!(p.toks.get(p.pos + 3).map(|t| &t.kind), Some(TokKind::Ident(_))) {
                return false;
            }
            if !matches!(p.toks.get(p.pos + 4).map(|t| &t.kind), Some(TokKind::LBrace)) {
                return false;
            }
            impl_decl(p, prog, true);
            true
        }
        _ => false,
    }
}

fn impl_decl(p: &mut Parser, prog: &mut Program, fuer: bool) {
    let start = p.bump(); // 'impl'
    if !p.pending_attrs.is_empty() {
        let sp = p.pending_attrs[0].span;
        p.dg.error_note(
            sp,
            "vor 'impl' ist kein attribut erlaubt".to_string(),
            "schreibe das attribut vor die einzelne methode".to_string(),
        );
        p.pending_attrs.clear();
    }
    let (erster, esp) = match p.ident("nach 'impl'") {
        Some(x) => x,
        None => {
            p.recovering = false;
            p.sync_item();
            return;
        }
    };
    // HOOK iface: `impl Schnittstelle for Typ` (iface.rs, Runde 46). Der
    // Block legt DIESELBEN Funktionen an wie `impl Typ` — die Schnittstelle
    // sagt nur zusaetzlich, was darin stehen MUSS.
    let (typ, tsp) = if fuer {
        p.bump(); // 'for'
        match p.ident("nach 'for' in 'impl … for …'") {
            Some(x) => x,
            None => {
                p.recovering = false;
                p.sync_item();
                return;
            }
        }
    } else {
        (erster.clone(), esp)
    };
    if fuer {
        crate::iface::merke_umsetzung(erster, typ.clone(), esp);
    }
    if !p.expect(TokKind::LBrace, "nach dem typnamen in 'impl'") {
        p.recovering = false;
        p.sync_item();
        return;
    }
    loop {
        while p.eat(&TokKind::Semi) {}
        if p.at(&TokKind::RBrace) || p.at_eof() {
            break;
        }
        if p.dg.is_full() {
            break;
        }
        let before = p.pos;
        if !p.at(&TokKind::KwFn) {
            p.error_here(format!(
                "erwartet 'fn' in einem impl-block, gefunden '{}'",
                p.kind().text()
            ));
            p.recovering = false;
            break;
        }
        // Eine kaputte Methode bricht den GANZEN Block ab. Sonst folgt auf
        // die eigentliche Meldung eine Kaskade aus Folgefehlern, und die
        // erste — die einzige, die etwas erklaert — geht darin unter.
        if !methode(p, prog, &typ, tsp) {
            p.recovering = false;
            p.sync_item();
            return;
        }
        if p.pos == before {
            p.bump();
        }
    }
    p.close(TokKind::RBrace, "am ende des impl-blocks");
    p.recovering = false;
    let _ = start;
}

/// Eine Methode: `fn name(<empfaenger>[, param…]) [-> T] { … }`.
/// `false` = abgebrochen, der umgebende `impl`-Block wird verworfen.
fn methode(p: &mut Parser, prog: &mut Program, typ: &str, tsp: Span) -> bool {
    let start = p.bump(); // 'fn'
    let name = match p.ident("nach 'fn' in einem impl-block") {
        Some((n, _)) => n,
        None => {
            p.recovering = false;
            p.sync_item();
            return false;
        }
    };
    if p.at(&TokKind::LBracket) {
        p.error_here("eine methode kann in dieser stufe nicht generisch sein");
        p.recovering = false;
        p.sync_item();
        return false;
    }
    if !p.expect(TokKind::LParen, "nach dem methodennamen") {
        p.recovering = false;
        p.sync_item();
        return false;
    }
    let selbst = match selbst_param(p, typ, tsp) {
        Some(x) => x,
        None => {
            p.recovering = false;
            p.sync_item();
            return false;
        }
    };
    let mut params = vec![selbst];
    if p.eat(&TokKind::Comma) {
        params.extend(p.params());
    }
    p.close(TokKind::RParen, "nach der parameterliste");
    p.recovering = false;
    let ret = if p.eat(&TokKind::Arrow) {
        match p.parse_type() {
            Some(t) => Some(t),
            None => {
                p.recovering = false;
                p.sync_item();
                return false;
            }
        }
    } else {
        None
    };
    if !p.at(&TokKind::LBrace) {
        p.error_here(format!(
            "erwartet '{{' am anfang des methodenrumpfes, gefunden '{}'",
            p.kind().text()
        ));
        p.recovering = false;
        p.sync_item();
        return false;
    }
    let body = p.block("am anfang des methodenrumpfes");
    p.recovering = false;
    let attrs = std::mem::take(&mut p.pending_attrs);
    prog.funcs.push(FnDecl {
        name: fn_name(typ, &name),
        params,
        ret,
        body,
        span: start,
        attrs,
    });
    true
}

/// Der Empfaenger: `self`, `*self` oder `*mut self`.
///
/// FUER EINE `gc class` (Runde 46) traegt der Empfaenger den INTERNEN
/// Structnamen `"gc K"`. Damit wird aus `*self` genau `Gc[K]` — ein
/// `gc class`-Wert existiert nur auf dem Heap, ein Zeiger darauf ist der
/// einzige Weg, ihn anzufassen (SPEC §3.5.1). Ob `K` eine Klasse ist, steht
/// in der Registrierung von `gc.rs`; sie wird beim Parsen gefuellt, deshalb
/// muss `gc class K` VOR dem `impl`-Block stehen (docs/RUNDE46.md §9).
/// Der interne Name wird von `modules.rs` nicht umbenannt — richtig so:
/// Klassennamen gelten programmweit.
fn selbst_param(p: &mut Parser, typ: &str, tsp: Span) -> Option<Param> {
    let klasse = crate::gc::ist_klasse(typ);
    let tname = if klasse {
        format!("gc {}", typ)
    } else {
        typ.to_string()
    };
    if matches!(p.kind(), TokKind::Ident(n) if n == "self") {
        let sp = p.bump();
        if klasse {
            p.dg.error_note(
                sp,
                format!("'{}' ist eine gc-klasse: der empfaenger kann keine kopie sein", typ),
                "ein 'gc class'-wert lebt nur auf dem GC-Heap: schreibe '*self' oder '*mut self'"
                    .to_string(),
            );
            return None;
        }
        return Some(Param {
            name: "self".to_string(),
            ty: TypeExpr::Named(tname, tsp),
            span: sp,
        });
    }
    if p.at(&TokKind::Star) {
        let star = p.span();
        if let Some((mutable, sp)) = zeiger_self(p) {
            return Some(Param {
                name: "self".to_string(),
                ty: TypeExpr::Ptr {
                    mutable,
                    inner: Box::new(TypeExpr::Named(tname, tsp)),
                    span: Parser::join(star, sp),
                },
                span: sp,
            });
        }
    }
    p.error_here(
        "der erste parameter einer methode ist der empfaenger: 'self', '*self' oder '*mut self'",
    );
    None
}

/// Verbraucht `*self` bzw. `*mut self` und liefert (veraenderlich, Position
/// von `self`). Steht danach kein `self`, wird nichts verbraucht.
pub(crate) fn zeiger_self(p: &mut Parser) -> Option<(bool, Span)> {
    let mit_mut = matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::KwMut));
    let idx = if mit_mut { p.pos + 2 } else { p.pos + 1 };
    if !matches!(p.toks.get(idx).map(|t| &t.kind), Some(TokKind::Ident(n)) if n == "self") {
        return None;
    }
    p.bump(); // '*'
    if mit_mut {
        p.bump(); // 'mut'
    }
    Some((mit_mut, p.bump())) // 'self'
}

/// `// HOOK impl` in `parser.rs::postfix` — `x.m(args)`.
///
/// Der Feldname ist bereits gelesen; folgt jetzt eine Klammer, ist es ein
/// Methodenaufruf und kein Feldzugriff. Ein qualifizierter Modulzugriff
/// (`modul.funktion(..)`) kommt hier nie an: den hat `Parser::qualify` in
/// `primary` schon zu EINEM Namen gemacht.
pub(crate) fn hook_methodenaufruf(
    p: &mut Parser,
    base: &Expr,
    name: &str,
    nsp: Span,
) -> Option<Expr> {
    // Die Klammer muss in DERSELBEN Zeile stehen. Ohne diese Frage waere
    //     let g: usize = (*p).kein_slit
    //     (*p).kein_slit = 0
    // ein Methodenaufruf `(*p).kein_slit((*p))` — der Zeilenumbruch beendet
    // die Anweisung (SPEC §10), und genau das prueft `cont`.
    if !p.cont() || !p.at(&TokKind::LParen) {
        return None;
    }
    p.bump(); // '('
    let (args, end) = p.call_args("nach der argumentliste eines methodenaufrufs");
    let mut alle = Vec::with_capacity(args.len() + 1);
    alle.push(base.clone());
    alle.extend(args);
    let span = Parser::join(base.span, end);
    Some(p.mk(span, ExprKind::Call(format!("{}{}", P_RUF, name), alle, nsp)))
}

// -------------------------------------------------------------- Typpruefung

/// Struktur hinter einem Empfaengertyp: `(Index, liegt schon als Zeiger vor)`.
/// Nur fuer die Frage „ist das ein `dyn I`?" — sonst gilt `empfaenger_praefix`.
fn empfaenger_struktur(tcx: &TypeCtx, t: &Type) -> Option<(usize, bool)> {
    match t {
        Type::Struct(i) if tcx.structs.get(*i).is_some() => Some((*i, false)),
        Type::Ptr { inner, .. } => match &**inner {
            Type::Struct(i) if tcx.structs.get(*i).is_some() => Some((*i, true)),
            _ => None,
        },
        _ => None,
    }
}

/// Der Name, unter dem die Methoden dieses Empfaengers stehen, und ob er
/// schon als Zeiger vorliegt: `(Praefix, ist_zeiger)`.
///
/// Seit Runde 50 zaehlt dazu auch ein GRUNDTYP (`impl Ord for i32` legt
/// `i32__kleiner` an). Ein typloses Ganzzahlliteral gehoert ausdruecklich
/// nicht dazu: `1.m()` haette keinen festen Typ, und welcher `impl`-Block
/// gemeint waere, koennte niemand sagen.
fn empfaenger_praefix(tcx: &TypeCtx, t: &Type) -> Option<(String, bool)> {
    fn name(tcx: &TypeCtx, t: &Type) -> Option<String> {
        match t {
            Type::Struct(i) if tcx.structs.get(*i).is_some() => {
                Some(crate::iface::methodenpraefix(tcx, *i))
            }
            other => crate::iface::grundtyp_name(other).map(|s| s.to_string()),
        }
    }
    match t {
        Type::Ptr { inner, .. } => name(tcx, inner).map(|n| (n, true)),
        other => name(tcx, other).map(|n| (n, false)),
    }
}

/// Schnittstelle hinter einem Empfaengertyp, wenn es ein `dyn I` ist.
/// (Runde 46; auch `sema::probe` fragt hier.)
pub(crate) fn dyn_schnittstelle(tcx: &TypeCtx, t: &Type) -> Option<String> {
    let (i, _) = empfaenger_struktur(tcx, t)?;
    let name = &tcx.structs.get(i)?.name;
    crate::iface::schnittstelle_von(name).map(|s| s.to_string())
}

/// Aufloesung: Zielfunktion und ob der Empfaenger als ADRESSE uebergeben
/// wird. EINE Stelle, drei Benutzer — `sema::probe` (Typhinweis),
/// `sema::call` (Pruefung) und `lower::lower_call` (Aufruf) rechnen alle
/// hiermit, damit sie nicht auseinanderlaufen koennen.
pub(crate) fn ziel_von(
    tcx: &TypeCtx,
    fns: &HashMap<String, FnSig>,
    methode: &str,
    empf: &Type,
) -> Option<(String, bool)> {
    let (praefix, ist_zeiger) = empfaenger_praefix(tcx, empf)?;
    let voll = fn_name(&praefix, methode);
    let sig = fns.get(&voll)?;
    let will_zeiger = sig.params.first().map(|t| t.is_ptr()).unwrap_or(false);
    Some((voll, will_zeiger && !ist_zeiger))
}

/// Dasselbe fuer das Lowering, das die fertige `TypeInfo` hat.
pub(crate) fn ziel(info: &TypeInfo, methode: &str, empf: &Type) -> Option<(String, bool)> {
    ziel_von(&info.tcx, &info.fns, methode, empf)
}

/// Kann von diesem Ausdruck eine Adresse genommen werden?
/// Dieselbe Menge, die `&x` erlaubt und die `lower::lower_addr` beherrscht.
fn ist_platz(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Ident(_) | ExprKind::Field(..) | ExprKind::Index(..) => true,
        ExprKind::Unary(op, _) => matches!(op, UnOp::Deref),
        _ => false,
    }
}

/// Alle Methoden eines Typs, alphabetisch — fuer die Fehlermeldung.
fn methoden_von(ck: &Checker, praefix: &str) -> Vec<String> {
    let praefix = format!("{}{}", praefix, TRENNER);
    let mut out: Vec<String> = ck
        .fns
        .keys()
        .filter_map(|k| k.strip_prefix(&praefix).map(|s| s.to_string()))
        .filter(|s| !s.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// `// HOOK impl` in `sema::Checker::call` — loest `x.m(args)` auf.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    let methode = methodenname(name)?.to_string();
    // Der Parser legt den Empfaenger immer als erstes Argument ab.
    let empf = match args.first() {
        Some(e) => e,
        None => return Some(Type::Error),
    };
    let et = ck.expr(empf, None);
    if et.is_error() {
        for a in &args[1..] {
            ck.type_out_expr(a);
        }
        return Some(Type::Error);
    }
    let (praefix, ist_zeiger) = match empfaenger_praefix(&ck.tcx, &et) {
        Some(x) => x,
        None => {
            for a in &args[1..] {
                ck.type_out_expr(a);
            }
            ck.dg.error_note(
                nspan,
                format!(
                    "methode '{}' auf einem wert vom typ {} — dieser typ kann keine methoden haben",
                    methode,
                    ck.tcx.name_of(&et)
                ),
                "eine methode wird mit 'impl Typ { fn … }' fuer einen struct- oder grundtyp vereinbart"
                    .to_string(),
            );
            return Some(Type::Error);
        }
    };
    // HOOK iface: `f.m(args)` auf einem `dyn I` — DYNAMISCHER VERSAND. Welche
    // Funktion laeuft, steht erst zur Laufzeit in der Methodentafel; geprueft
    // wird gegen die Schnittstelle (iface.rs, Runde 46).
    if let Some((sidx, _)) = empfaenger_struktur(&ck.tcx, &et) {
        let sname = ck.tcx.structs[sidx].name.clone();
        if let Some(iname) = crate::iface::schnittstelle_von(&sname) {
            let iname = iname.to_string();
            return Some(crate::iface::hook_methode(
                ck, &iname, &methode, args, &et, ist_zeiger, nspan, espan,
            ));
        }
    }
    let sname = praefix.clone();
    let voll = fn_name(&praefix, &methode);
    let sig = match ck.fns.get(&voll) {
        Some(s) => s.clone(),
        None => {
            for a in &args[1..] {
                ck.type_out_expr(a);
            }
            let vorhanden = methoden_von(ck, &praefix);
            let note = if vorhanden.is_empty() {
                format!("fuer '{}' ist kein 'impl'-block vereinbart", sname)
            } else {
                format!("'{}' hat: {}", sname, vorhanden.join(", "))
            };
            ck.dg.error_note(
                nspan,
                format!("typ '{}' hat keine methode '{}'", sname, methode),
                note,
            );
            return Some(Type::Error);
        }
    };
    let anzeige = format!("{}.{}", praefix, methode);
    // Empfaenger anpassen — die einzige Automatik am Aufrufort.
    match sig.params.first() {
        Some(t) if t.is_ptr() && !ist_zeiger => {
            if !ist_platz(empf) {
                ck.dg.error_note(
                    empf.span,
                    format!(
                        "der empfaenger von '{}' braucht eine adresse, dieser ausdruck hat keine",
                        anzeige
                    ),
                    "binde ihn an eine variable und rufe die methode darauf auf".to_string(),
                );
            }
        }
        Some(t) if !t.is_ptr() && ist_zeiger => {
            ck.dg.error_note(
                empf.span,
                format!(
                    "'{}' erwartet den empfaenger als wert, gefunden {}",
                    anzeige,
                    ck.tcx.name_of(&et)
                ),
                format!("schreibe (*x).{}(…), wenn die kopie gemeint ist", methode),
            );
        }
        _ => {}
    }
    // Die uebrigen Argumente — gezaehlt wird OHNE den Empfaenger.
    let erwartet = sig.params.len().saturating_sub(1);
    let gefunden = args.len().saturating_sub(1);
    if gefunden != erwartet {
        ck.dg.error(
            espan,
            format!(
                "methode '{}' erwartet {} argument(e), gefunden {}",
                anzeige, erwartet, gefunden
            ),
        );
    }
    for (i, a) in args[1..].iter().enumerate() {
        match sig.params.get(i + 1) {
            Some(p) => ck.pruefe_argument(&anzeige, i + 1, a, p),
            None => ck.type_out_expr(a),
        }
    }
    Some(sig.ret)
}
