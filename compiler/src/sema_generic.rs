//! Generische Vorlagen (`L5`): Erfassung im Parser, Namensschema, Anforderungen.
//!
//! Diese Datei gehoert dem Modul `types`. Generics werden **monomorphisiert**
//! (siehe `mono.rs`): fuer jede benutzte Typkombination entsteht eine eigene,
//! vollstaendig konkrete Funktion bzw. ein eigener Struct. Das Namensschema ist
//! Vertrag (Debugger, Inlining, Tests):
//!
//! ```text
//! name__T1_T2      z. B.  vec_push__i32, Vec__ptr_u8, Map__u32_i64
//! ```
//!
//! Syntax (eckige Klammern, damit `<` eindeutig Vergleich bleibt — SPEC §12):
//!
//! ```firn
//! struct Vec[T] { data: *mut T, len: usize, cap: usize }
//! fn summe[T: Int](a: T, b: T) -> T { return a + b }
//! let s = summe[i32](1 as i32, 2 as i32)
//! var v: Vec[i32] = Vec[i32]{ data: p, len: 0 as usize, cap: 0 as usize }
//! ```
//!
//! Anforderungen (`T: Int`) werden bei der Monomorphisierung geprueft; ein
//! nicht erfuelltes `T` ist ein Fehler mit Zeile und Spalte.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ast::{Expr, ExprKind, FnDecl, StructDecl, TypeExpr};
use crate::diag::Span;
use crate::lexer::{TokKind, Token};
use crate::parser::Parser;

/// Schranke an einem Typparameter — `T: Int`, `T: Ord`, `T: Int + Ord`.
///
/// `Any`, `Int` und `Scalar` sind die drei EINGEBAUTEN Schranken (Runde 30).
/// Jeder andere Name ist der Name einer SCHNITTSTELLE (Runde 50). Ob es diese
/// Schnittstelle gibt, steht beim Parsen noch nicht fest: `interface Ord` darf
/// weiter unten oder in einer anderen Datei stehen. Geprueft wird deshalb bei
/// der AUSPRAEGUNG (`mono.rs`) — dort, wo der konkrete Typ bekannt ist und die
/// Meldung sagen kann, welche Methode fehlt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Bound {
    /// keine Anforderung
    Any,
    /// Ganzzahltyp
    Int,
    /// Ganzzahl, bool oder Zeiger
    Scalar,
    /// `T: I` — der Typ muss die Schnittstelle `I` umsetzen
    Iface(String),
}

impl Bound {
    pub(crate) fn name(&self) -> &str {
        match self {
            Bound::Any => "Any",
            Bound::Int => "Int",
            Bound::Scalar => "Scalar",
            Bound::Iface(n) => n.as_str(),
        }
    }
    /// Kein `Option`: ein unbekannter Name ist keine Fehleingabe, sondern der
    /// Name einer Schnittstelle. Ein Tippfehler wird bei der Auspraegung als
    /// „unbekannte schnittstelle" gemeldet — mit der Liste der bekannten.
    fn parse(name: &str) -> Bound {
        match name {
            "Any" => Bound::Any,
            "Int" => Bound::Int,
            "Scalar" => Bound::Scalar,
            _ => Bound::Iface(name.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TyParam {
    pub(crate) name: String,
    /// leere Liste = keine Schranke (`[T]`)
    pub(crate) bounds: Vec<Bound>,
}

#[derive(Clone, Debug)]
pub(crate) struct FnTemplate {
    pub(crate) params: Vec<TyParam>,
    pub(crate) decl: FnDecl,
}

#[derive(Clone, Debug)]
pub(crate) struct StructTemplate {
    pub(crate) params: Vec<TyParam>,
    pub(crate) decl: StructDecl,
}

/// Eine benutzte Typkombination (`Vec[i32]`, `summe[u8]`).
#[derive(Clone, Debug)]
pub(crate) struct Instantiation {
    pub(crate) base: String,
    pub(crate) args: Vec<TypeExpr>,
    pub(crate) span: Span,
    /// innerhalb einer Vorlage aufgeschrieben (enthaelt evtl. Typparameter)
    pub(crate) abstrakt: bool,
    pub(crate) is_fn: bool,
}

#[derive(Default)]
struct Registry {
    fn_names: Vec<String>,
    struct_names: Vec<String>,
    fns: HashMap<String, FnTemplate>,
    structs: HashMap<String, StructTemplate>,
    insts: HashMap<String, Instantiation>,
    order: Vec<String>,
    /// Verschachtelungstiefe beim Parsen einer Vorlage
    in_template: u32,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

pub(crate) fn reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

pub(crate) fn fn_template(name: &str) -> Option<FnTemplate> {
    REG.with(|r| r.borrow().fns.get(name).cloned())
}

/// Namen aller generischen Vorlagen, die in Datei `file` deklariert wurden.
///
/// Gebraucht von `modules.rs`: die Vorlagen liegen NICHT in `Program::funcs`,
/// sondern hier — das Modul-Umschreiben erreichte sie deshalb nie, und eine
/// Vorlage sah nur die Namen der Wurzeldatei
/// (docs/SELBSTHOSTING.md §7, Blocker B2).
pub(crate) fn fn_vorlagen_der_datei(file: u32) -> Vec<String> {
    REG.with(|r| {
        r.borrow()
            .fns
            .iter()
            .filter(|(_, t)| t.decl.span.file == file)
            .map(|(n, _)| n.clone())
            .collect()
    })
}

pub(crate) fn struct_vorlagen_der_datei(file: u32) -> Vec<String> {
    REG.with(|r| {
        r.borrow()
            .structs
            .iter()
            .filter(|(_, t)| t.decl.span.file == file)
            .map(|(n, _)| n.clone())
            .collect()
    })
}

/// Aendert eine Funktionsvorlage an Ort und Stelle.
pub(crate) fn mit_fn_vorlage<F: FnOnce(&mut crate::ast::FnDecl)>(name: &str, f: F) {
    REG.with(|r| {
        if let Some(t) = r.borrow_mut().fns.get_mut(name) {
            f(&mut t.decl);
        }
    });
}

/// Aendert eine Structvorlage an Ort und Stelle.
pub(crate) fn mit_struct_vorlage<F: FnOnce(&mut crate::ast::StructDecl)>(name: &str, f: F) {
    REG.with(|r| {
        if let Some(t) = r.borrow_mut().structs.get_mut(name) {
            f(&mut t.decl);
        }
    });
}

pub(crate) fn struct_template(name: &str) -> Option<StructTemplate> {
    REG.with(|r| r.borrow().structs.get(name).cloned())
}

/// Wird gerade der Rumpf einer generischen Vorlage geparst?
pub(crate) fn in_template() -> bool {
    REG.with(|r| r.borrow().in_template > 0)
}

pub(crate) fn is_generic_fn(name: &str) -> bool {
    REG.with(|r| r.borrow().fn_names.iter().any(|n| n == name))
}

pub(crate) fn is_generic_struct(name: &str) -> bool {
    REG.with(|r| r.borrow().struct_names.iter().any(|n| n == name))
}

/// Alle beim Parsen erfassten Verwendungen, in Reihenfolge des Auftretens.
pub(crate) fn instantiations() -> Vec<(String, Instantiation)> {
    REG.with(|r| {
        let reg = r.borrow();
        reg.order
            .iter()
            .filter_map(|k| reg.insts.get(k).map(|i| (k.clone(), i.clone())))
            .collect()
    })
}

pub(crate) fn instantiation(mangled: &str) -> Option<Instantiation> {
    REG.with(|r| r.borrow().insts.get(mangled).cloned())
}

fn record_inst(mangled: &str, inst: Instantiation) {
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        if !reg.insts.contains_key(mangled) {
            reg.order.push(mangled.to_string());
            reg.insts.insert(mangled.to_string(), inst);
        }
    });
}

// ------------------------------------------------------------ Namensschema

/// Textform eines Typs fuer das Namensschema `name__T1_T2`.
pub(crate) fn type_tag(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Named(n, _) => n.clone(),
        TypeExpr::Ptr { mutable, inner, .. } => {
            format!("{}{}", if *mutable { "ptrmut_" } else { "ptr_" }, type_tag(inner))
        }
        TypeExpr::Array { elem, len, .. } => format!("arr{}_{}", len, type_tag(elem)),
    }
}

/// `name__T1_T2` (Vertrag).
pub(crate) fn mangle(base: &str, args: &[TypeExpr]) -> String {
    let mut s = String::from(base);
    s.push_str("__");
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            s.push('_');
        }
        s.push_str(&type_tag(a));
    }
    s
}

// ------------------------------------------------------------- Parser-Hooks

impl<'a> Parser<'a> {
    /// `[T, U: Int]` — Typparameterliste einer Vorlage.
    fn generic_params(&mut self) -> Option<Vec<TyParam>> {
        if !self.expect(TokKind::LBracket, "am anfang der typparameterliste") {
            return None;
        }
        let mut out: Vec<TyParam> = Vec::new();
        loop {
            if self.at(&TokKind::RBracket) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (name, span) = self.ident("fuer einen typparameter")?;
            // `T: A + B + C` — die Schranken stehen mit `+` hintereinander und
            // gelten ALLE gleichzeitig (Runde 50).
            let mut bounds: Vec<Bound> = Vec::new();
            if self.eat(&TokKind::Colon) {
                loop {
                    let (bname, bspan) = self.ident("fuer eine schranke am typparameter")?;
                    let b = Bound::parse(&bname);
                    if bounds.contains(&b) {
                        self.dg.error_note(
                            bspan,
                            format!("die schranke '{}' steht zweimal an '{}'", bname, name),
                            "jede schranke wird hoechstens einmal genannt",
                        );
                    } else {
                        bounds.push(b);
                    }
                    if !self.eat(&TokKind::Plus) {
                        break;
                    }
                }
            }
            if out.iter().any(|p| p.name == name) {
                self.dg
                    .error(span, format!("typparameter '{}' ist bereits deklariert", name));
            } else {
                let _ = span;
                out.push(TyParam { name, bounds });
            }
            if !self.eat(&TokKind::Comma) {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.close(TokKind::RBracket, "nach der typparameterliste");
        if out.is_empty() {
            self.error_here("eine typparameterliste braucht mindestens einen parameter");
            self.recovering = false;
            return None;
        }
        Some(out)
    }

    /// `[i32, u8]` — Typargumente an einer Verwendungsstelle.
    fn generic_args(&mut self) -> Option<Vec<TypeExpr>> {
        if !self.expect(TokKind::LBracket, "am anfang der typargumente") {
            return None;
        }
        let mut out = Vec::new();
        loop {
            if self.at(&TokKind::RBracket) || self.at_eof() {
                break;
            }
            let before = self.pos;
            out.push(self.parse_type()?);
            if !self.eat(&TokKind::Comma) {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.close(TokKind::RBracket, "nach den typargumenten");
        if out.is_empty() {
            self.error_here("es fehlen die typargumente");
            self.recovering = false;
            return None;
        }
        Some(out)
    }

    fn generic_fn_template(&mut self) {
        let start = self.span();
        self.bump(); // 'fn'
        let (name, nspan) = match self.ident("nach 'fn'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        let tparams = match self.generic_params() {
            Some(p) => p,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        REG.with(|r| r.borrow_mut().in_template += 1);
        let decl = self.rest_of_fn(name.clone(), start);
        REG.with(|r| r.borrow_mut().in_template -= 1);
        let decl = match decl {
            Some(d) => d,
            None => return,
        };
        let doppelt = REG.with(|r| {
            let mut reg = r.borrow_mut();
            if reg.fns.contains_key(&name) {
                return true;
            }
            reg.fns.insert(name.clone(), FnTemplate { params: tparams, decl });
            if !reg.fn_names.contains(&name) {
                reg.fn_names.push(name.clone());
            }
            false
        });
        if doppelt {
            self.dg.error(
                nspan,
                format!("generische funktion '{}' ist bereits deklariert", name),
            );
        }
    }

    fn rest_of_fn(&mut self, name: String, start: Span) -> Option<FnDecl> {
        if !self.expect(TokKind::LParen, "nach dem funktionsnamen") {
            self.recovering = false;
            self.sync_item();
            return None;
        }
        let params = self.params();
        self.close(TokKind::RParen, "nach der parameterliste");
        self.recovering = false;
        let ret = if self.eat(&TokKind::Arrow) {
            match self.parse_type() {
                Some(t) => Some(t),
                None => {
                    self.recovering = false;
                    self.sync_item();
                    return None;
                }
            }
        } else {
            None
        };
        if !self.at(&TokKind::LBrace) {
            self.error_here(format!(
                "erwartet '{{' am anfang des funktionsrumpfes, gefunden '{}'",
                self.kind().text()
            ));
            self.recovering = false;
            self.sync_item();
            return None;
        }
        let body = self.block("am anfang des funktionsrumpfes");
        self.recovering = false;
        Some(FnDecl { name, params, ret, body, span: start, attrs: Vec::new() })
    }

    fn generic_struct_template(&mut self) {
        let start = self.span();
        self.bump(); // 'struct'
        let (name, nspan) = match self.ident("nach 'struct'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        let tparams = match self.generic_params() {
            Some(p) => p,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::LBrace, "nach dem strukturnamen") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        REG.with(|r| r.borrow_mut().in_template += 1);
        let mut fields: Vec<(String, TypeExpr, Span)> = Vec::new();
        loop {
            while self.eat(&TokKind::Comma) || self.eat(&TokKind::Semi) {}
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (fname, fspan) = match self.ident("fuer ein feld") {
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
            fields.push((fname, ty, fspan));
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.close(TokKind::RBrace, "am ende der strukturdeklaration");
        self.recovering = false;
        REG.with(|r| r.borrow_mut().in_template -= 1);
        let decl =
            StructDecl { name: name.clone(), fields, span: Parser::join(start, end), attrs: Vec::new() };
        let doppelt = REG.with(|r| {
            let mut reg = r.borrow_mut();
            if reg.structs.contains_key(&name) {
                return true;
            }
            reg.structs
                .insert(name.clone(), StructTemplate { params: tparams, decl });
            if !reg.struct_names.contains(&name) {
                reg.struct_names.push(name.clone());
            }
            false
        });
        if doppelt {
            self.dg.error(
                nspan,
                format!("generischer struct '{}' ist bereits deklariert", name),
            );
        }
    }

    fn note_inst(&mut self, base: &str, args: Vec<TypeExpr>, span: Span, is_fn: bool) -> String {
        let mangled = mangle(base, &args);
        let abstrakt = REG.with(|r| r.borrow().in_template > 0);
        record_inst(
            &mangled,
            Instantiation { base: base.to_string(), args, span, abstrakt, is_fn },
        );
        mangled
    }
}

/// Vorabsuche im Tokenstrom: welche Namen sind generische Vorlagen? Damit
/// funktioniert die Benutzung auch VOR der Deklaration.
pub(crate) fn hook_prescan(toks: &[Token]) {
    let mut fns: Vec<String> = Vec::new();
    let mut sts: Vec<String> = Vec::new();
    for i in 0..toks.len() {
        let k = &toks[i].kind;
        let is_fn = matches!(k, TokKind::KwFn);
        let is_st = matches!(k, TokKind::KwStruct);
        if !is_fn && !is_st {
            continue;
        }
        let name = match toks.get(i + 1).map(|t| &t.kind) {
            Some(TokKind::Ident(n)) => n.clone(),
            _ => continue,
        };
        if !matches!(toks.get(i + 2).map(|t| &t.kind), Some(TokKind::LBracket)) {
            continue;
        }
        if is_fn {
            fns.push(name);
        } else {
            sts.push(name);
        }
    }
    // Additiv: bei mehreren Quelldateien (modules.rs) kommen die Namen jeder
    // Datei hinzu; zurueckgesetzt wird nur einmal je Uebersetzung.
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        for n in fns {
            if !reg.fn_names.contains(&n) {
                reg.fn_names.push(n);
            }
        }
        for n in sts {
            if !reg.struct_names.contains(&n) {
                reg.struct_names.push(n);
            }
        }
    });
}

/// `// HOOK types` in `parser.rs::program` (ueber `sema_match::hook_item`).
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    let name = match p.toks.get(p.pos + 1).map(|t| t.kind.clone()) {
        Some(TokKind::Ident(n)) => n,
        _ => return false,
    };
    let _ = name;
    if !matches!(p.toks.get(p.pos + 2).map(|t| &t.kind), Some(TokKind::LBracket)) {
        return false;
    }
    match p.kind() {
        TokKind::KwFn => {
            p.generic_fn_template();
            true
        }
        TokKind::KwStruct => {
            p.generic_struct_template();
            true
        }
        _ => false,
    }
}

/// `// HOOK types` in `parser.rs::parse_type_inner` — `Vec[i32]`.
pub(crate) fn hook_generic_type(p: &mut Parser, name: &str, sp: Span) -> Option<TypeExpr> {
    if !p.at(&TokKind::LBracket) || !is_generic_struct(name) {
        return None;
    }
    let args = p.generic_args()?;
    let mangled = p.note_inst(name, args, sp, false);
    Some(TypeExpr::Named(mangled, sp))
}

/// `// HOOK types` in `parser.rs::postfix` — `summe[i32](a, b)`.
pub(crate) fn hook_generic_call(p: &mut Parser, base: &Expr) -> Option<Expr> {
    let name = match &base.kind {
        ExprKind::Ident(n) if is_generic_fn(n) => n.clone(),
        _ => return None,
    };
    let args = p.generic_args()?;
    let mangled = p.note_inst(&name, args, base.span, true);
    if !p.at(&TokKind::LParen) {
        p.error_here(format!(
            "erwartet '(' nach den typargumenten von '{}', gefunden '{}'",
            name,
            p.kind().text()
        ));
        p.recovering = false;
        return None;
    }
    p.bump();
    let (cargs, end) = p.call_args("nach der argumentliste");
    let span = Parser::join(base.span, end);
    Some(p.mk(span, ExprKind::Call(mangled, cargs, base.span)))
}

/// `// HOOK types` in `parser.rs::primary` — `Vec[i32]{ .. }`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    let name = match p.kind().clone() {
        TokKind::Ident(n) if is_generic_struct(&n) => n,
        _ => return None,
    };
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::LBracket)) {
        return None;
    }
    let sp = p.span();
    p.bump();
    let args = p.generic_args()?;
    let mangled = p.note_inst(&name, args, sp, false);
    if p.at(&TokKind::LBrace) && !p.no_struct_lit {
        return Some(p.struct_lit(mangled, sp));
    }
    p.error_here(format!(
        "erwartet '{{' nach '{}[..]', gefunden '{}'",
        name,
        p.kind().text()
    ));
    p.recovering = false;
    None
}
