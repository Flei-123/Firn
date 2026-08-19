//! `interface` und **dynamischer Versand** — Runde 46 (SPEC §6.2).
//!
//! ## Was diese Runde hinzufuegt
//!
//! Runde 45 brachte `impl Typ { fn m(*self) … }` — eine reine Schreibhilfe:
//! `x.m(a)` wurde zu `Typ__m(&x, a)`, entschieden allein vom **statischen**
//! Typ. Diese Runde ergaenzt das um den anderen Fall, den SPEC §6.2
//! ausdruecklich verlangt: **eine Aufrufstelle, viele Typen**.
//!
//! ```text
//! interface Flaeche {              // Vertrag: Methoden ohne Rumpf
//!     fn flaeche(*self) -> i64
//!     fn skaliere(*mut self, k: i64)
//! }
//!
//! impl Flaeche for Rechteck { … }  // Umsetzung, vollstaendig geprueft
//!
//! let f: dyn Flaeche = (&r) as dyn Flaeche   // Schnittstellenwert
//! f.flaeche()                                // ueber die Methodentafel
//! ```
//!
//! ## Die Darstellung — verbindlich
//!
//! ```text
//! interface I      -> Struct "dyn I" in types::TypeCtx, 16 Byte:
//!                       daten: *mut u8   (Versatz 0) — der Wert selbst
//!                       tafel: *mut u8   (Versatz 8) — die Methodentafel
//! dyn I            -> genau dieser Struct (ein FETTER ZEIGER, kein Zeiger)
//! impl I for T     -> Methodentafel `.L__iface.I.T` in `.rodata`:
//!                       .quad T__m1
//!                       .quad T__m2      (Reihenfolge = Reihenfolge in `I`)
//! ```
//!
//! Der interne Structname traegt ein **Leerzeichen** (`"dyn I"`) — dieselbe
//! Bauart wie `"gc C"` in `gc.rs` und `"methode m"` in `impls.rs`. Er kann aus
//! keinem Bezeichner des Quelltextes entstehen, und `TypeCtx::name_of` gibt
//! ihn unveraendert aus: in jeder Fehlermeldung steht `dyn I`, so wie es im
//! Quelltext geschrieben wird.
//!
//! ## Warum ein Struct und kein eigener `Type`
//!
//! Ein `Type::Dyn(..)` haette jede Fallunterscheidung ueber `Type` angefasst —
//! Layout, ABI, Monomorphisierung, Optimierer, Debuginfo. Als Struct ist der
//! Schnittstellenwert ein **gewoehnliches 16-Byte-Aggregat**: `abi::classify`
//! gibt ihm zwei INTEGER-Woerter, er wird kopiert, uebergeben, zurueckgegeben
//! und im Rahmen abgelegt wie jeder andere Struct. Die ganze Sprachaenderung
//! sitzt in Parser, Typpruefer und zwei FIR-Instruktionen.
//!
//! ## Warum der Sammler den Wert dahinter weiter findet
//!
//! Der Datenzeiger ist ein **gewoehnlicher Zeiger auf den Anfang** des Wertes
//! — nicht verschleiert (anders als `GcWeak`, `gc.rs`), nicht auf ein Feld
//! versetzt. Ein `dyn I` liegt im Rahmen oder in einem callee-saved Register;
//! beides durchsucht der Sammler konservativ (SPEC §3.5.3), und beim
//! Sammellauf rettet `Op::GcAddr { regs: true }` die Register vorher in den
//! Zustandsblock. Damit haelt ein Schnittstellenwert sein Objekt am Leben,
//! auch wenn es sonst keine Wurzel mehr gibt (`tests/823_iface_gc_kern.fi`).
//!
//! Die **eine** Stelle, an der das nicht traegt, ist der Heap: dort verfolgt
//! der Sammler PRAEZISE anhand des Feldlayouts und wuerde den Datenzeiger in
//! einem `dyn I`-Feld nicht kennen. Deshalb ist `dyn I` als Feld einer
//! `gc class` ein Fehler (`iface_dyn_in_gc_klasse.fi`) — ein Loch in einer
//! Zusage waere schlimmer als eine fehlende Bequemlichkeit.

use std::cell::RefCell;

use crate::ast::{Expr, TypeExpr};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::{Type, TypeCtx};

/// Praefix des internen Structnamens eines Schnittstellenwertes.
/// Das Leerzeichen macht ihn unerreichbar fuer den Quelltext.
pub(crate) const P_DYN: &str = "dyn ";
/// Praefix der Methodentafeln im Assembler (dateilokal, `.L`).
const TAFEL_LABEL: &str = ".L__iface.";
/// Versatz des Datenzeigers im Schnittstellenwert.
pub(crate) const OFF_DATEN: u64 = 0;
/// Versatz der Methodentafel im Schnittstellenwert.
pub(crate) const OFF_TAFEL: u64 = 8;

// ---------------------------------------------------------------- Datenmodell

#[derive(Clone, Debug)]
struct Methode {
    name: String,
    /// Parameter OHNE den Empfaenger.
    params: Vec<TypeExpr>,
    ret: Option<TypeExpr>,
    /// `*mut self` statt `*self` — heute Absicht, kein Zwang (impls.rs).
    veraenderlich: bool,
    span: Span,
    /// nach `hook_check_impls`: aufgeloeste Typen
    ptypen: Vec<Type>,
    rtyp: Type,
    /// Typen bereits aufgeloest (Nachtraege aus `comptime` melden sonst doppelt)
    aufgeloest: bool,
}

#[derive(Clone, Debug)]
struct Schnittstelle {
    name: String,
    methoden: Vec<Methode>,
    /// Index des Structs `"dyn I"` in `TypeCtx`, `usize::MAX` bis zur Anmeldung
    struct_idx: usize,
}

#[derive(Clone, Debug)]
struct Umsetzung {
    iface: String,
    /// Typname, wie er im Quelltext steht (VOR der Modulumbenennung).
    typ: String,
    span: Span,
    /// nach der Pruefung: Index des Structs, `usize::MAX` = nicht aufgeloest
    struct_idx: usize,
    /// nach der Pruefung: Name, unter dem die Methoden dieses Typs stehen
    /// (`T__m`) — also der ENDGUELTIGE Name nach der Modulumbenennung, ohne
    /// das interne `"gc "` einer Klasse. Der Codegenerator hat die Typtabelle
    /// nicht mehr; deshalb steht er hier.
    praefix: String,
    /// nach der Pruefung: `true`, wenn die Umsetzung vollstaendig ist
    ok: bool,
    /// schon geprueft (Nachtraege aus `comptime` melden sonst doppelt)
    geprueft: bool,
}

#[derive(Default)]
struct Registry {
    ifaces: Vec<Schnittstelle>,
    impls: Vec<Umsetzung>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

/// Setzt die Registrierung zurueck (eine je Uebersetzung, `parser::reset_hooks`).
pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

fn iface_index(name: &str) -> Option<usize> {
    REG.with(|r| r.borrow().ifaces.iter().position(|i| i.name == name))
}

/// Gibt es ueberhaupt eine Schnittstelle in dieser Uebersetzung?
pub(crate) fn hat_schnittstellen() -> bool {
    REG.with(|r| !r.borrow().ifaces.is_empty())
}

/// Ist `sname` der interne Name eines Schnittstellenwertes? Liefert den
/// Schnittstellennamen.
pub(crate) fn schnittstelle_von(sname: &str) -> Option<&str> {
    sname.strip_prefix(P_DYN)
}

/// Name des Structs hinter `dyn I`.
fn dyn_name(iface: &str) -> String {
    format!("{}{}", P_DYN, iface)
}

/// Ist dieser Typ ein Schnittstellenwert?
pub(crate) fn ist_dyn(tcx: &TypeCtx, t: &Type) -> bool {
    match t {
        Type::Struct(i) => tcx
            .structs
            .get(*i)
            .map(|s| s.name.starts_with(P_DYN))
            .unwrap_or(false),
        _ => false,
    }
}

/// Schluessel der Methodentafel: `<Schnittstelle>.<Typ>`.
fn tafelschluessel(iface: &str, typname: &str) -> String {
    format!("{}.{}", iface, typname)
}

// ------------------------------------------------------------------- Parser

/// `// HOOK iface` in `parser.rs::program` — `interface Name { … }`.
///
/// `interface` ist KEIN Schluesselwort (der Tokenisierer kennt es nicht),
/// sondern ein Bezeichner in einer Stellung, in der sonst nichts stehen darf —
/// dieselbe Loesung wie `gc class` (gc.rs) und `impl` (impls.rs). Damit bleibt
/// `interface` als Variablenname gueltig.
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    if !matches!(p.kind(), TokKind::Ident(n) if n == "interface") {
        return false;
    }
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::Ident(_))) {
        return false;
    }
    if !matches!(p.toks.get(p.pos + 2).map(|t| &t.kind), Some(TokKind::LBrace)) {
        return false;
    }
    interface_decl(p);
    true
}

fn interface_decl(p: &mut Parser) {
    p.bump(); // 'interface'
    if !p.pending_attrs.is_empty() {
        let sp = p.pending_attrs[0].span;
        p.dg.error_note(
            sp,
            "vor 'interface' ist kein attribut erlaubt".to_string(),
            "attribute an schnittstellen gibt es in dieser stufe nicht".to_string(),
        );
        p.pending_attrs.clear();
    }
    let (name, nsp) = match p.ident("nach 'interface'") {
        Some(x) => x,
        None => {
            p.recovering = false;
            p.sync_item();
            return;
        }
    };
    if !p.expect(TokKind::LBrace, "nach dem namen der schnittstelle") {
        p.recovering = false;
        p.sync_item();
        return;
    }
    let mut methoden: Vec<Methode> = Vec::new();
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
                "erwartet 'fn' in einer schnittstelle, gefunden '{}'",
                p.kind().text()
            ));
            p.recovering = false;
            p.sync_item();
            return;
        }
        // Eine kaputte Methode bricht die GANZE Schnittstelle ab — dieselbe
        // Regel wie im `impl`-Block (impls.rs): die erste Meldung ist die
        // einzige, die etwas erklaert.
        match methodenkopf(p, &name) {
            Some(m) => {
                if methoden.iter().any(|x| x.name == m.name) {
                    p.dg.error(
                        m.span,
                        format!(
                            "die schnittstelle '{}' hat die methode '{}' bereits",
                            name, m.name
                        ),
                    );
                }
                methoden.push(m);
            }
            None => {
                p.recovering = false;
                p.sync_item();
                return;
            }
        }
        if p.pos == before {
            p.bump();
        }
    }
    p.close(TokKind::RBrace, "am ende der schnittstelle");
    p.recovering = false;
    if iface_index(&name).is_some() {
        p.dg.error(
            nsp,
            format!("die schnittstelle '{}' ist bereits deklariert", name),
        );
        return;
    }
    REG.with(|r| {
        r.borrow_mut().ifaces.push(Schnittstelle {
            name,
            methoden,
            struct_idx: usize::MAX,
        })
    });
}

/// Eine Methode der Schnittstelle: `fn name(<empfaenger>[, param…]) [-> T]`
/// — ohne Rumpf.
fn methodenkopf(p: &mut Parser, iface: &str) -> Option<Methode> {
    p.bump(); // 'fn'
    let (name, nsp) = match p.ident("nach 'fn' in einer schnittstelle") {
        Some(x) => x,
        None => return None,
    };
    if p.at(&TokKind::LBracket) {
        p.error_here("eine schnittstellenmethode kann in dieser stufe nicht generisch sein");
        return None;
    }
    if !p.expect(TokKind::LParen, "nach dem methodennamen") {
        return None;
    }
    // DER EMPFAENGER MUSS EIN ZEIGER SEIN. Ueber die Methodentafel steht nur
    // der Datenzeiger zur Verfuegung — eine Kopie des Wertes koennte der
    // Aufrufer gar nicht bilden, er kennt den konkreten Typ nicht.
    let veraenderlich = if p.at(&TokKind::Star) {
        match crate::impls::zeiger_self(p) {
            Some((m, _)) => m,
            None => {
                p.error_here(format!(
                    "der empfaenger einer schnittstellenmethode ist '*self' oder '*mut self' ('{}.{}')",
                    iface, name
                ));
                return None;
            }
        }
    } else {
        p.error_here(format!(
            "der empfaenger einer schnittstellenmethode ist '*self' oder '*mut self' ('{}.{}')",
            iface, name
        ));
        return None;
    };
    let mut params: Vec<TypeExpr> = Vec::new();
    if p.eat(&TokKind::Comma) {
        params = p.params().into_iter().map(|x| x.ty).collect();
    }
    p.close(TokKind::RParen, "nach der parameterliste");
    p.recovering = false;
    let ret = if p.eat(&TokKind::Arrow) {
        match p.parse_type() {
            Some(t) => Some(t),
            None => return None,
        }
    } else {
        None
    };
    if p.at(&TokKind::LBrace) {
        p.error_here(format!(
            "eine schnittstellenmethode hat keinen rumpf ('{}.{}')",
            iface, name
        ));
        return None;
    }
    Some(Methode {
        name,
        params,
        ret,
        veraenderlich,
        span: nsp,
        ptypen: Vec::new(),
        rtyp: Type::Void,
        aufgeloest: false,
    })
}

/// `// HOOK iface` in `parser.rs::parse_type_inner` — `dyn I`.
///
/// Der Typname wird zu EINEM Namen mit Leerzeichen zusammengezogen; aufgeloest
/// wird er im Typpruefer, der die Structtabelle kennt.
pub(crate) fn hook_type(p: &mut Parser, name: &str, sp: Span) -> Option<TypeExpr> {
    if name != "dyn" {
        return None;
    }
    let n = match p.kind() {
        TokKind::Ident(n) => n.clone(),
        _ => return None,
    };
    let end = p.bump();
    Some(TypeExpr::Named(dyn_name(&n), Parser::join(sp, end)))
}

/// `// HOOK iface` in `impls.rs::impl_decl` — `impl I for T { … }` anmelden.
pub(crate) fn merke_umsetzung(iface: String, typ: String, span: Span) {
    REG.with(|r| {
        r.borrow_mut().impls.push(Umsetzung {
            iface,
            typ,
            span,
            struct_idx: usize::MAX,
            praefix: String::new(),
            ok: false,
            geprueft: false,
        })
    });
}

// -------------------------------------------------------------- Typpruefung

/// `// HOOK iface` in `sema::add_items_inner` (VOR `collect_structs`): fuer
/// jede Schnittstelle den Struct `"dyn I"` anlegen.
///
/// Zweimal aufgerufen zu werden ist erlaubt (`comptime`-Nachtraege): eine
/// bereits angemeldete Schnittstelle wird uebersprungen.
pub(crate) fn declare_interfaces(ck: &mut Checker) {
    let n = REG.with(|r| r.borrow().ifaces.len());
    for i in 0..n {
        let (name, fertig) = REG.with(|r| {
            let reg = r.borrow();
            let s = &reg.ifaces[i];
            (s.name.clone(), s.struct_idx != usize::MAX)
        });
        if fertig {
            continue;
        }
        let sidx = ck.tcx.declare(&dyn_name(&name));
        ck.tcx.set_fields(
            sidx,
            vec![
                ("daten".to_string(), Type::ptr(Type::U8, true)),
                ("tafel".to_string(), Type::ptr(Type::U8, true)),
            ],
        );
        REG.with(|r| r.borrow_mut().ifaces[i].struct_idx = sidx);
    }
}

/// `// HOOK iface` in `sema::resolve_ty_d` — `dyn I` mit unbekanntem `I`.
pub(crate) fn hook_resolve_ty(ck: &mut Checker, te: &TypeExpr) -> Option<Type> {
    let (name, span) = match te {
        TypeExpr::Named(n, s) => (n.as_str(), *s),
        _ => return None,
    };
    let iface = schnittstelle_von(name)?;
    if ck.tcx.lookup(name).is_some() {
        return None; // die gewoehnliche Aufloesung findet den Struct
    }
    ck.dg.error_note(
        span,
        format!("unbekannte schnittstelle '{}'", iface),
        format!("eine schnittstelle wird mit 'interface {} {{ … }}' vereinbart", iface),
    );
    Some(Type::Error)
}

/// Der Name, unter dem die Methoden eines Typs stehen: `T__m`.
/// Fuer eine `gc class` ist das der Klassenname OHNE das interne `"gc "`.
pub(crate) fn methodenpraefix(tcx: &TypeCtx, idx: usize) -> String {
    match tcx.structs.get(idx) {
        Some(s) => s.name.strip_prefix("gc ").unwrap_or(&s.name).to_string(),
        None => String::new(),
    }
}

/// Vergleich zweier Typen an einer Signaturgrenze — dieselbe Regel wie
/// `sema::compatible`: Zeiger werden OHNE die Veraenderlichkeit verglichen
/// (`*T` und `*mut T` sind in dieser Sprache fuereinander einsetzbar).
fn passt(a: &Type, b: &Type) -> bool {
    if a.is_error() || b.is_error() {
        return true;
    }
    match (a, b) {
        (Type::Ptr { inner: x, .. }, Type::Ptr { inner: y, .. }) => passt(x, y),
        _ => a == b,
    }
}

/// Struct hinter dem Typnamen einer Umsetzung.
///
/// Der Name steht so in der Registrierung, wie er im Quelltext geschrieben
/// wurde — die Modulumbenennung (`modules.rs`) laeuft NACH dem Parsen und
/// fasst die Registrierung nicht an. Deshalb wird hier in drei Schritten
/// gesucht: der Name selbst, die `gc class` desselben Namens, und zuletzt
/// GENAU EIN Struct, dessen Name auf `__<Name>` endet (das ist der Fall
/// „Typ in einem Modul"). Mehrere Treffer sind ein Fehler — raten waere die
/// gefaehrlichere Wahl.
fn typ_struct(ck: &Checker, name: &str) -> Result<usize, bool> {
    if let Some(i) = ck.tcx.lookup(name) {
        return Ok(i);
    }
    if let Some(i) = ck.tcx.lookup(&format!("gc {}", name)) {
        return Ok(i);
    }
    let suffix = format!("__{}", name);
    let treffer: Vec<usize> = ck
        .tcx
        .structs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.ends_with(&suffix))
        .map(|(i, _)| i)
        .collect();
    match treffer.len() {
        1 => Ok(treffer[0]),
        0 => Err(false),
        _ => Err(true),
    }
}

/// Lesbare Signatur einer Schnittstellenmethode (fuer die Fehlermeldung).
fn signatur(ck: &Checker, m: &Methode) -> String {
    let mut s = String::from(if m.veraenderlich { "*mut self" } else { "*self" });
    for t in &m.ptypen {
        s.push_str(", ");
        s.push_str(&ck.tcx.name_of(t));
    }
    match &m.rtyp {
        Type::Void => format!("fn {}({})", m.name, s),
        r => format!("fn {}({}) -> {}", m.name, s, ck.tcx.name_of(r)),
    }
}

/// `// HOOK iface` in `sema::add_items_inner` (NACH `collect_fns`): prueft
/// jede Umsetzung vollstaendig — alle Methoden da, alle Signaturen passend.
pub(crate) fn hook_check_impls(ck: &mut Checker) {
    // 1. Typen der Schnittstellenmethoden aufloesen (einmal je Methode).
    let n = REG.with(|r| r.borrow().ifaces.len());
    for i in 0..n {
        let anz = REG.with(|r| r.borrow().ifaces[i].methoden.len());
        for k in 0..anz {
            let (fertig, params, ret) = REG.with(|r| {
                let reg = r.borrow();
                let m = &reg.ifaces[i].methoden[k];
                (m.aufgeloest, m.params.clone(), m.ret.clone())
            });
            if fertig {
                continue;
            }
            let pt: Vec<Type> = params.iter().map(|t| ck.resolve_ty(t)).collect();
            let rt = match &ret {
                Some(t) => ck.resolve_ty(t),
                None => Type::Void,
            };
            REG.with(|r| {
                let mut reg = r.borrow_mut();
                let m = &mut reg.ifaces[i].methoden[k];
                m.ptypen = pt;
                m.rtyp = rt;
                m.aufgeloest = true;
            });
        }
    }
    // 2. Jede Umsetzung pruefen.
    let anz = REG.with(|r| r.borrow().impls.len());
    for u in 0..anz {
        let (iface, typ, span, geprueft) = REG.with(|r| {
            let reg = r.borrow();
            let x = &reg.impls[u];
            (x.iface.clone(), x.typ.clone(), x.span, x.geprueft)
        });
        if geprueft {
            continue;
        }
        REG.with(|r| r.borrow_mut().impls[u].geprueft = true);
        pruefe_umsetzung(ck, u, &iface, &typ, span);
    }
    // 3. Kein Schnittstellenwert im Heap: der Sammler verfolgt dort PRAEZISE
    //    (SPEC §3.5.3) und kennt den Datenzeiger in einem `dyn I` nicht.
    pruefe_gc_felder(ck);
}

fn pruefe_umsetzung(ck: &mut Checker, u: usize, iface: &str, typ: &str, span: Span) {
    let ii = match iface_index(iface) {
        Some(i) => i,
        None => {
            let bekannt: Vec<String> =
                REG.with(|r| r.borrow().ifaces.iter().map(|s| s.name.clone()).collect());
            let note = if bekannt.is_empty() {
                "in dieser uebersetzung ist keine schnittstelle vereinbart".to_string()
            } else {
                format!("bekannt sind: {}", bekannt.join(", "))
            };
            ck.dg.error_note(
                span,
                format!("unbekannte schnittstelle '{}'", iface),
                note,
            );
            return;
        }
    };
    let sidx = match typ_struct(ck, typ) {
        Ok(i) => i,
        Err(mehrdeutig) => {
            if mehrdeutig {
                ck.dg.error_note(
                    span,
                    format!("der typ '{}' ist mehrdeutig", typ),
                    "mehrere module deklarieren einen typ dieses namens".to_string(),
                );
            } else {
                ck.dg.error(span, format!("unbekannter typ '{}'", typ));
            }
            return;
        }
    };
    if ck.tcx.structs[sidx].name.starts_with(P_DYN) {
        ck.dg.error_note(
            span,
            format!("'{}' ist eine schnittstelle und kein typ", typ),
            "eine schnittstelle setzt keine schnittstelle um".to_string(),
        );
        return;
    }
    // Doppelte Umsetzung — verglichen wird der AUFGELOESTE Struct, damit
    // `impl I for T` und `impl I for modul.T` als dasselbe erkannt werden.
    let doppelt = REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .take(u)
            .any(|x| x.iface == iface && x.struct_idx == sidx)
    });
    if doppelt {
        ck.dg.error(
            span,
            format!("'{}' setzt die schnittstelle '{}' bereits um", typ, iface),
        );
        return;
    }
    let praefix = methodenpraefix(&ck.tcx, sidx);
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        reg.impls[u].struct_idx = sidx;
        reg.impls[u].praefix = praefix.clone();
    });
    let anzeige = ck.tcx.structs[sidx].name.clone();
    let methoden = REG.with(|r| r.borrow().ifaces[ii].methoden.clone());
    let mut vollstaendig = true;
    for m in &methoden {
        let voll = format!("{}{}{}", praefix, crate::impls::TRENNER, m.name);
        let sig = match ck.fns.get(&voll) {
            Some(s) => s.clone(),
            None => {
                vollstaendig = false;
                ck.dg.error_note(
                    span,
                    format!(
                        "'{}' setzt die methode '{}.{}' nicht um",
                        anzeige, iface, m.name
                    ),
                    format!("erwartet wird '{}' im block", signatur(ck, m)),
                );
                continue;
            }
        };
        // Empfaenger: ein Zeiger auf GENAU diesen Typ.
        let empf_ok = match sig.params.first() {
            Some(Type::Ptr { inner, .. }) => **inner == Type::Struct(sidx),
            _ => false,
        };
        if !empf_ok {
            vollstaendig = false;
            ck.dg.error_note(
                span,
                format!(
                    "der empfaenger von '{}.{}' passt nicht zu '{}'",
                    anzeige, m.name, iface
                ),
                format!("erwartet wird '{}' im block", signatur(ck, m)),
            );
            continue;
        }
        if sig.params.len() != m.ptypen.len() + 1 {
            vollstaendig = false;
            ck.dg.error_note(
                span,
                format!(
                    "'{}.{}' hat {} parameter, die schnittstelle '{}' verlangt {}",
                    anzeige,
                    m.name,
                    sig.params.len() - 1,
                    iface,
                    m.ptypen.len()
                ),
                format!("erwartet wird '{}' im block", signatur(ck, m)),
            );
            continue;
        }
        let mut passend = true;
        for (k, erwartet) in m.ptypen.iter().enumerate() {
            let ist = &sig.params[k + 1];
            if !passt(ist, erwartet) {
                passend = false;
                ck.dg.error_note(
                    span,
                    format!(
                        "parameter {} von '{}.{}' hat typ {}, die schnittstelle '{}' verlangt {}",
                        k + 1,
                        anzeige,
                        m.name,
                        ck.tcx.name_of(ist),
                        iface,
                        ck.tcx.name_of(erwartet)
                    ),
                    format!("erwartet wird '{}' im block", signatur(ck, m)),
                );
                break;
            }
        }
        if !passend {
            vollstaendig = false;
            continue;
        }
        if !passt(&sig.ret, &m.rtyp) {
            vollstaendig = false;
            ck.dg.error_note(
                span,
                format!(
                    "'{}.{}' liefert {}, die schnittstelle '{}' verlangt {}",
                    anzeige,
                    m.name,
                    ck.tcx.name_of(&sig.ret),
                    iface,
                    ck.tcx.name_of(&m.rtyp)
                ),
                format!("erwartet wird '{}' im block", signatur(ck, m)),
            );
        }
    }
    REG.with(|r| r.borrow_mut().impls[u].ok = vollstaendig);
}

/// Enthaelt `t` DEM WERT NACH einen Schnittstellenwert?
fn enthaelt_dyn(tcx: &TypeCtx, t: &Type, tiefe: u32) -> bool {
    if tiefe > 32 {
        return false;
    }
    match t {
        Type::Struct(i) => match tcx.structs.get(*i) {
            Some(s) if s.name.starts_with(P_DYN) => true,
            Some(s) => s
                .fields
                .iter()
                .any(|f| enthaelt_dyn(tcx, &f.ty, tiefe + 1)),
            None => false,
        },
        Type::Array(e, _) => enthaelt_dyn(tcx, e, tiefe + 1),
        _ => false,
    }
}

/// `dyn I` im Heap waere ein Loch in der Sammlerzusage (siehe Kopf der Datei).
fn pruefe_gc_felder(ck: &mut Checker) {
    if !hat_schnittstellen() {
        return;
    }
    let mut befunde: Vec<(String, String)> = Vec::new();
    for s in ck.tcx.structs.iter() {
        let klasse = match s.name.strip_prefix("gc ") {
            Some(k) => k,
            None => continue,
        };
        for f in &s.fields {
            if enthaelt_dyn(&ck.tcx, &f.ty, 0) {
                befunde.push((klasse.to_string(), f.name.clone()));
            }
        }
    }
    for (klasse, feld) in befunde {
        ck.dg.error_note(
            Span::none(),
            format!(
                "feld '{}' der gc-klasse '{}' enthaelt einen schnittstellenwert",
                feld, klasse
            ),
            "der sammler verfolgt den heap praezise und kennt den zeiger hinter 'dyn' nicht (SPEC 3.5.3)".to_string(),
        );
    }
}

/// `// HOOK iface` in `sema::expr_inner`, Zweig `Cast` — `x as dyn I`.
///
/// Der Schnittstellenwert entsteht AUSDRUECKLICH (SPEC §6.2: „dynamische
/// Aufloesung ueber `dyn Interface`, ausdruecklich hingeschrieben"). Es gibt
/// keine stille Umwandlung an einer Zuweisung oder an einem Argument — Firn
/// hat keine impliziten Umwandlungen (SPEC §4.5), und fuer eine, die eine
/// Methodentafel anhaengt, waere das die schlechteste Stelle anzufangen.
pub(crate) fn hook_cast(ck: &mut Checker, span: Span, src: &Type, dst: &Type) -> Option<Type> {
    let sidx = match dst {
        Type::Struct(i) => *i,
        _ => return None,
    };
    let iface = ck
        .tcx
        .structs
        .get(sidx)
        .and_then(|s| schnittstelle_von(&s.name))?
        .to_string();
    let quelle = match src {
        Type::Ptr { inner, .. } => match &**inner {
            Type::Struct(j) => *j,
            _ => {
                ck.dg.error_note(
                    span,
                    format!(
                        "ein schnittstellenwert entsteht aus einem zeiger auf einen struct, gefunden {}",
                        ck.tcx.name_of(src)
                    ),
                    format!("schreibe '(&x) as dyn {}'", iface),
                );
                return Some(Type::Error);
            }
        },
        _ => {
            ck.dg.error_note(
                span,
                format!(
                    "ein schnittstellenwert entsteht aus einem zeiger, gefunden {}",
                    ck.tcx.name_of(src)
                ),
                format!("schreibe '(&x) as dyn {}'", iface),
            );
            return Some(Type::Error);
        }
    };
    if umsetzung_ok(&iface, quelle) {
        return Some(dst.clone());
    }
    let name = ck.tcx.name_of(&Type::Struct(quelle));
    let bekannt = REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .filter(|x| x.iface == iface && x.struct_idx != usize::MAX)
            .map(|x| x.typ.clone())
            .collect::<Vec<_>>()
    });
    let note = if bekannt.is_empty() {
        format!("kein typ setzt '{}' um", iface)
    } else {
        format!("'{}' setzen um: {}", iface, bekannt.join(", "))
    };
    ck.dg.error_note(
        span,
        format!("'{}' setzt die schnittstelle '{}' nicht um", name, iface),
        note,
    );
    Some(Type::Error)
}

/// Setzt der Struct `sidx` die Schnittstelle `iface` vollstaendig um?
fn umsetzung_ok(iface: &str, sidx: usize) -> bool {
    REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .any(|x| x.iface == iface && x.struct_idx == sidx && x.ok)
    })
}

/// Nummer der Methode in der Schnittstelle (= Platz in der Methodentafel).
pub(crate) fn slot_von(iface: &str, methode: &str) -> Option<usize> {
    let i = iface_index(iface)?;
    REG.with(|r| {
        r.borrow().ifaces[i]
            .methoden
            .iter()
            .position(|m| m.name == methode)
    })
}

/// Rueckgabetyp einer Schnittstellenmethode — auch fuer `sema::probe`, damit
/// ein Literal daneben seinen Typ bekommt (`f.flaeche() != 42`).
pub(crate) fn ret_von(iface: &str, methode: &str) -> Option<Type> {
    let i = iface_index(iface)?;
    REG.with(|r| {
        r.borrow().ifaces[i]
            .methoden
            .iter()
            .find(|m| m.name == methode)
            .map(|m| m.rtyp.clone())
    })
}

/// `// HOOK iface` in `impls::hook_call` — `f.m(args)` auf einem `dyn I`.
///
/// Der Empfaenger ist ein WERT (der fette Zeiger selbst). Ein `*dyn I` wird
/// bewusst nicht angenommen: `(*z).m(…)` sagt dasselbe und macht sichtbar,
/// dass zwei Woerter gelesen werden.
pub(crate) fn hook_methode(
    ck: &mut Checker,
    iface: &str,
    methode: &str,
    args: &[Expr],
    et: &Type,
    ist_zeiger: bool,
    nspan: Span,
    espan: Span,
) -> Type {
    let anzeige = format!("dyn {}.{}", iface, methode);
    let ii = match iface_index(iface) {
        Some(i) => i,
        None => return Type::Error,
    };
    let m = match REG.with(|r| {
        r.borrow().ifaces[ii]
            .methoden
            .iter()
            .find(|m| m.name == methode)
            .cloned()
    }) {
        Some(m) => m,
        None => {
            for a in &args[1..] {
                ck.type_out_expr(a);
            }
            let vorhanden: Vec<String> = REG.with(|r| {
                r.borrow().ifaces[ii]
                    .methoden
                    .iter()
                    .map(|m| m.name.clone())
                    .collect()
            });
            let note = if vorhanden.is_empty() {
                format!("die schnittstelle '{}' hat keine methoden", iface)
            } else {
                format!("'{}' hat: {}", iface, vorhanden.join(", "))
            };
            ck.dg.error_note(
                nspan,
                format!("die schnittstelle '{}' hat keine methode '{}'", iface, methode),
                note,
            );
            return Type::Error;
        }
    };
    if ist_zeiger {
        if let Some(empf) = args.first() {
            ck.dg.error_note(
                empf.span,
                format!(
                    "'{}' erwartet den schnittstellenwert selbst, gefunden {}",
                    anzeige,
                    ck.tcx.name_of(et)
                ),
                format!("schreibe (*x).{}(…)", methode),
            );
        }
    }
    let erwartet = m.ptypen.len();
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
        match m.ptypen.get(i) {
            Some(p) => ck.pruefe_argument(&anzeige, i + 1, a, p),
            None => ck.type_out_expr(a),
        }
    }
    m.rtyp
}

// ------------------------------------------------------------------ Lowering

/// Methodentafel einer Umsetzung — der Schluessel fuer `Op::VtabAddr`.
/// `None`, wenn dieser Typ die Schnittstelle nicht (vollstaendig) umsetzt.
pub(crate) fn tafel_von(iface: &str, sidx: usize) -> Option<String> {
    REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .find(|x| x.iface == iface && x.struct_idx == sidx && x.ok)
            .map(|x| tafelschluessel(iface, &x.praefix))
    })
}

// ------------------------------------------------------------- Codegenerator

/// Alle Methodentafeln als `.rodata`-Block.
///
/// Eine Tafel je vollstaendiger Umsetzung — auch fuer eine, die nie in einem
/// `as dyn` vorkommt. Das ist Absicht: der Inhalt haengt allein an der
/// Deklaration, nicht an den Aufrufstellen; eine Tafel, die nur manchmal
/// entsteht, waere die Sorte Zustand, die man beim Fehlersuchen nicht sehen
/// will.
pub(crate) fn tafeln_asm() -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let tafeln: Vec<(String, Vec<String>)> = REG.with(|r| {
        let reg = r.borrow();
        reg.impls
            .iter()
            .filter(|u| u.ok && u.struct_idx != usize::MAX)
            .filter_map(|u| {
                let ii = reg.ifaces.iter().position(|s| s.name == u.iface)?;
                let ziele: Vec<String> = reg.ifaces[ii]
                    .methoden
                    .iter()
                    .map(|m| format!("{}{}{}", u.praefix, crate::impls::TRENNER, m.name))
                    .collect();
                Some((tafelschluessel(&u.iface, &u.praefix), ziele))
            })
            .collect()
    });
    if tafeln.is_empty() {
        return out;
    }
    let _ = writeln!(out, ".section .rodata");
    let _ = writeln!(out, ".align 8");
    for (schluessel, ziele) in tafeln {
        let _ = writeln!(out, "{}{}:", TAFEL_LABEL, schluessel);
        for z in ziele {
            let _ = writeln!(out, "    .quad {}", crate::codegen_x86::label(&z));
        }
    }
    out
}

/// Signatur einer Schnittstellenmethode fuer das Lowering: der Empfaenger
/// zaehlt als erster Parameter (ein Zeiger), danach die Parameter aus der
/// Deklaration. Damit sieht der Aufruf fuer `lower_call` genauso aus wie
/// jeder andere.
fn methoden_sig(iface: &str, methode: &str) -> Option<crate::sema::FnSig> {
    let i = iface_index(iface)?;
    REG.with(|r| {
        let reg = r.borrow();
        let m = reg.ifaces[i].methoden.iter().find(|m| m.name == methode)?;
        let mut params = vec![Type::ptr(Type::U8, true)];
        params.extend(m.ptypen.iter().cloned());
        Some(crate::sema::FnSig { params, ret: m.rtyp.clone() })
    })
}

/// `// HOOK iface` in `lower::lower_call` — den dynamischen Versand
/// vorbereiten: Datenzeiger lesen, Methodentafel lesen, Eintrag lesen.
///
/// ```text
/// %b = <adresse des schnittstellenwertes>
/// %d = load.ptr [%b + 0]      ; der Wert selbst
/// %t = load.ptr [%b + 8]      ; die Methodentafel
/// %z = load.ptr [%t + 8*k]    ; die k-te Methode der Schnittstelle
/// ```
///
/// Drei Ladebefehle je Aufruf — das ist der Preis des dynamischen Versands,
/// und er steht hier sichtbar (SPEC §1, Leitsatz 1: nichts versteckt).
pub(crate) fn lower_versand(
    lo: &mut crate::lower::Lower,
    iface: &str,
    methode: &str,
    empf: &Expr,
    span: Span,
) -> Option<(crate::fir::Val, crate::fir::Val, crate::sema::FnSig)> {
    use crate::fir::{FTy, Op};
    let slot = match slot_von(iface, methode) {
        Some(s) => s,
        None => return lo.ice(span, "unbekannte schnittstellenmethode im lowering"),
    };
    let sig = match methoden_sig(iface, methode) {
        Some(s) => s,
        None => return lo.ice(span, "schnittstellenmethode ohne signatur im lowering"),
    };
    let basis = lo.lower_addr(empf)?;
    let dadr = lo.field_addr_at(basis, OFF_DATEN);
    let daten = lo.load(FTy::Ptr, dadr);
    let tadr = lo.field_addr_at(basis, OFF_TAFEL);
    let tafel = lo.load(FTy::Ptr, tadr);
    let eadr = lo.field_addr_at(tafel, 8 * slot as u64);
    let ziel = lo.load(FTy::Ptr, eadr);
    Some((ziel, daten, sig))
}

/// `// HOOK iface` in `lower::write_into_inner` — `p as dyn I`.
///
/// Zwei Woerter: der Zeiger, wie er ist, und die Adresse der Methodentafel.
/// Der Datenzeiger wird NICHT veraendert (kein Versatz, keine Verschleierung)
/// — nur so findet der konservative Stapelscan des Sammlers das Objekt
/// dahinter (SPEC §3.5.3, siehe Kopf dieser Datei).
pub(crate) fn lower_cast_into(
    lo: &mut crate::lower::Lower,
    addr: crate::fir::Val,
    inner: &Expr,
    t: &Type,
    span: Span,
) -> Option<()> {
    use crate::fir::{FTy, Op};
    let sidx = match t {
        Type::Struct(i) => *i,
        _ => return lo.ice(span, "schnittstellenwert ohne struct-typ"),
    };
    let iface = match lo.info.tcx.structs.get(sidx).and_then(|s| schnittstelle_von(&s.name)) {
        Some(i) => i.to_string(),
        None => return lo.ice(span, "umwandlung in einen nicht-schnittstellentyp"),
    };
    let quelle = match lo.ty_of(inner) {
        Type::Ptr { inner, .. } => match *inner {
            Type::Struct(j) => j,
            _ => return lo.ice(span, "schnittstellenwert aus einem zeiger ohne struct"),
        },
        _ => return lo.ice(span, "schnittstellenwert aus einem nicht-zeiger"),
    };
    let schluessel = match tafel_von(&iface, quelle) {
        Some(k) => k,
        None => return lo.ice(span, "umsetzung ohne methodentafel im lowering"),
    };
    let pv = lo.lower_expr(inner)?;
    let dadr = lo.field_addr_at(addr, OFF_DATEN);
    lo.store(FTy::Ptr, dadr, pv);
    let tv = lo.push(FTy::Ptr, Op::VtabAddr { tafel: schluessel });
    let tadr = lo.field_addr_at(addr, OFF_TAFEL);
    lo.store(FTy::Ptr, tadr, tv);
    Some(())
}

/// Assemblername einer Methodentafel.
pub(crate) fn tafel_label(schluessel: &str) -> String {
    format!("{}{}", TAFEL_LABEL, schluessel)
}
