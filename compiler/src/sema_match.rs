//! Summentypen (`enum`) und Musterabgleich (`match`) — SPEC §6.3, `L4`.
//!
//! Diese Datei gehoert dem Modul `types`. Sie enthaelt
//!  * die Parser-Erweiterungen (als `impl` auf `parser::Parser`, angebunden an
//!    die `// HOOK types`-Zeilen in `parser.rs`),
//!  * die Registrierung der Aufzaehlungen im Typkontext,
//!  * die Typpruefung der Muster und
//!  * die **Vollstaendigkeitspruefung zur Uebersetzungszeit**
//!    (`check_exhaustive`) — ein fehlender Fall ist ein FEHLER mit Zeile und
//!    Spalte und nennt die fehlende Variante, kein Warnhinweis.
//!
//! ## Speicherlayout einer Aufzaehlung (verbindlich)
//!
//! ```text
//! offset 0        : __tag : u32      (Variantennummer, 0-basiert, Deklarationsreihenfolge)
//! offset payload_off : Nutzdaten der jeweiligen Variante
//! ```
//!
//! `payload_off = round_up(4, payload_align)`, wobei `payload_align` die
//! groesste Ausrichtung aller Nutzdatenfelder ist (mindestens 1). Die
//! Nutzdatenfelder einer Variante liegen in Deklarationsreihenfolge mit
//! natuerlicher Ausrichtung hintereinander; die Bereiche **verschiedener**
//! Varianten ueberlagern sich (echte Vereinigung). Groesse der Aufzaehlung =
//! `round_up(payload_off + max_variantengroesse, align)`,
//! `align = max(4, payload_align)`.
//!
//! Technisch wird die Aufzaehlung als Struct mit den Feldern `__tag` und
//! `__v<tag>_<i>` in `types::TypeCtx` eingetragen; die Offsets werden hier
//! berechnet, nicht von `TypeCtx::set_fields`.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ast::{Block, Expr, ExprKind, Stmt, TypeExpr};
use crate::diag::{Diag, Span};
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::{round_up, Field, StructDef, Type};

/// Praefix der internen Aufruf-Namen fuer `match`-Anweisungen. Enthaelt `#`,
/// kann also nie ein Bezeichner aus dem Quelltext sein.
pub(crate) const MATCH_PREFIX: &str = "__match#";

// ---------------------------------------------------------------- Datenmodell

#[derive(Clone, Debug)]
pub(crate) struct VariantDef {
    pub(crate) name: String,
    pub(crate) tag: i128,
    /// Nutzdatentypen wie geschrieben
    pub(crate) field_tys: Vec<TypeExpr>,
    /// aufgeloeste Nutzdatentypen (nach `layout_enums`)
    pub(crate) fields: Vec<Type>,
    /// Byte-Offsets der Nutzdatenfelder (nach `layout_enums`)
    pub(crate) offsets: Vec<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct EnumDef {
    pub(crate) name: String,
    pub(crate) span: Span,
    pub(crate) variants: Vec<VariantDef>,
    /// Index in `TypeCtx::structs`
    pub(crate) struct_idx: usize,
    pub(crate) size: u64,
    pub(crate) align: u64,
}

impl EnumDef {
    pub(crate) fn variant(&self, name: &str) -> Option<&VariantDef> {
        self.variants.iter().find(|v| v.name == name)
    }
}

/// Muster (SPEC §6.3): Variante mit Bindung, Literal, Bereich, `_`, verschachtelt.
#[derive(Clone, Debug)]
pub(crate) enum Pattern {
    /// `_`
    Wild(Span),
    /// `name` — bindet den ganzen Wert
    Bind(String, Span),
    Int(i128, Span),
    Bool(bool, Span),
    /// `lo..hi` (halboffen) bzw. `lo..=hi` (einschliessend)
    Range { lo: i128, hi: i128, inclusive: bool, span: Span },
    /// `Enum::Variante(unter, muster)` — `ename` kann fehlen (`::Variante`)
    Variant { ename: Option<String>, vname: String, subs: Vec<Pattern>, span: Span },
}

impl Pattern {
    pub(crate) fn span(&self) -> Span {
        match self {
            Pattern::Wild(s) | Pattern::Bind(_, s) | Pattern::Int(_, s) | Pattern::Bool(_, s) => *s,
            Pattern::Range { span, .. } => *span,
            Pattern::Variant { span, .. } => *span,
        }
    }
    /// Trifft das Muster IMMER zu (bindet also nur)?
    pub(crate) fn is_irrefutable(&self) -> bool {
        matches!(self, Pattern::Wild(_) | Pattern::Bind(..))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Arm {
    pub(crate) pat: Pattern,
    pub(crate) body: Block,
    pub(crate) span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct MatchInfo {
    pub(crate) subject: Expr,
    pub(crate) arms: Vec<Arm>,
    pub(crate) span: Span,
}

/// Was der Musterabgleich untersucht — Grundlage der Vollstaendigkeitspruefung.
#[derive(Clone, Debug)]
pub(crate) enum Subject {
    Enum(EnumDef),
    Bool,
    Int(Type),
    /// Fehlerhafter Ausdruck — es wurde bereits gemeldet.
    Bad,
}

#[derive(Default)]
struct Registry {
    enums: Vec<EnumDef>,
    by_name: HashMap<String, usize>,
    by_struct: HashMap<usize, usize>,
    matches: Vec<MatchInfo>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

/// Setzt alle Registrierungen zurueck (eine je Uebersetzung).
pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
    crate::sema_generic::reset();
}

pub(crate) fn enum_count() -> usize {
    REG.with(|r| r.borrow().enums.len())
}

pub(crate) fn enum_at(i: usize) -> Option<EnumDef> {
    REG.with(|r| r.borrow().enums.get(i).cloned())
}

pub(crate) fn enum_by_name(name: &str) -> Option<EnumDef> {
    REG.with(|r| {
        let reg = r.borrow();
        reg.by_name.get(name).and_then(|i| reg.enums.get(*i)).cloned()
    })
}

pub(crate) fn enum_by_struct(idx: usize) -> Option<EnumDef> {
    REG.with(|r| {
        let reg = r.borrow();
        reg.by_struct.get(&idx).and_then(|i| reg.enums.get(*i)).cloned()
    })
}

pub(crate) fn is_enum_name(name: &str) -> bool {
    REG.with(|r| r.borrow().by_name.contains_key(name))
}

pub(crate) fn match_info(idx: usize) -> Option<MatchInfo> {
    REG.with(|r| r.borrow().matches.get(idx).cloned())
}

fn match_index_of(name: &str) -> Option<usize> {
    name.strip_prefix(MATCH_PREFIX).and_then(|s| s.parse::<usize>().ok())
}

// ------------------------------------------------------------- Parser-Hooks

impl<'a> Parser<'a> {
    fn tk(&self, off: usize) -> TokKind {
        match self.toks.get(self.pos + off) {
            Some(t) => t.kind.clone(),
            None => TokKind::Eof,
        }
    }

    fn tspan(&self, off: usize) -> Span {
        match self.toks.get(self.pos + off) {
            Some(t) => t.span,
            None => Span::none(),
        }
    }

    /// Stehen die Token `off` und `off+1` unmittelbar nebeneinander?
    fn adjacent(&self, off: usize) -> bool {
        let a = self.tspan(off);
        let b = self.tspan(off + 1);
        a.line == b.line && a.col + a.len == b.col
    }

    /// `::`
    pub(crate) fn types_at_colon2(&self, off: usize) -> bool {
        self.tk(off) == TokKind::Colon && self.tk(off + 1) == TokKind::Colon && self.adjacent(off)
    }

    /// `=>`
    pub(crate) fn types_at_fat_arrow(&self) -> bool {
        self.tk(0) == TokKind::Assign && self.tk(1) == TokKind::Gt && self.adjacent(0)
    }

    /// `..`
    fn types_at_dotdot(&self) -> bool {
        self.tk(0) == TokKind::DotDot
    }

    fn types_eat_colon2(&mut self) {
        self.bump();
        self.bump();
    }

    /// Aufzaehlungsdeklaration: `enum Name { A, B(i32), C(Point, bool) }`
    fn types_enum_decl(&mut self) {
        let start = self.bump(); // 'enum'
        let (name, nspan) = match self.ident("nach 'enum'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::LBrace, "nach dem namen der aufzaehlung") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let mut variants: Vec<VariantDef> = Vec::new();
        loop {
            while self.eat(&TokKind::Comma) {}
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (vname, vspan) = match self.ident("fuer eine variante der aufzaehlung") {
                Some(x) => x,
                None => break,
            };
            let mut field_tys = Vec::new();
            if self.eat(&TokKind::LParen) {
                loop {
                    if self.at(&TokKind::RParen) || self.at_eof() {
                        break;
                    }
                    let b2 = self.pos;
                    match self.parse_type() {
                        Some(t) => field_tys.push(t),
                        None => break,
                    }
                    if !self.eat(&TokKind::Comma) {
                        break;
                    }
                    if self.pos == b2 {
                        self.bump();
                    }
                }
                self.close(TokKind::RParen, "nach den nutzdaten einer variante");
            }
            if variants.iter().any(|v| v.name == vname) {
                self.dg.error(
                    vspan,
                    format!("variante '{}' ist in aufzaehlung '{}' bereits deklariert", vname, name),
                );
            } else {
                let tag = variants.len() as i128;
                variants.push(VariantDef {
                    name: vname,
                    tag,
                    field_tys,
                    fields: Vec::new(),
                    offsets: Vec::new(),
                });
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.close(TokKind::RBrace, "am ende der aufzaehlung");
        self.recovering = false;
        if variants.is_empty() {
            self.dg.error(
                nspan,
                format!("aufzaehlung '{}' hat keine variante", name),
            );
            return;
        }
        let def = EnumDef {
            name: name.clone(),
            span: Parser::join(start, end),
            variants,
            struct_idx: usize::MAX,
            size: 0,
            align: 1,
        };
        let doppelt = REG.with(|r| {
            let mut reg = r.borrow_mut();
            if reg.by_name.contains_key(&name) {
                return true;
            }
            let i = reg.enums.len();
            reg.enums.push(def);
            reg.by_name.insert(name.clone(), i);
            false
        });
        if doppelt {
            self.dg
                .error(nspan, format!("aufzaehlung '{}' ist bereits deklariert", name));
        }
    }

    /// `match subjekt { muster => { .. } .. }` als Anweisung.
    fn types_match_stmt(&mut self) -> Stmt {
        let start = self.bump(); // 'match'
        if crate::sema_generic::in_template() {
            // Die Rumpfbloecke der Faelle liegen in der Registrierung, nicht im
            // AST — eine Vorlage koennte sie nicht je Auspraegung ersetzen.
            self.dg.error_note(
                start,
                "'match' innerhalb einer generischen vorlage wird in dieser stufe nicht unterstuetzt",
                "lagere den musterabgleich in eine nicht generische funktion aus",
            );
        }
        let saved = self.no_struct_lit;
        self.no_struct_lit = true;
        let subject = self.expr();
        self.no_struct_lit = saved;
        if !self.expect(TokKind::LBrace, "nach dem ausdruck von 'match'") {
            self.recovering = false;
            self.sync_item();
            return Stmt::Error(start);
        }
        let mut arms: Vec<Arm> = Vec::new();
        loop {
            while self.eat(&TokKind::Comma) || self.eat(&TokKind::Semi) {}
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let pat = match self.types_pattern(0) {
                Some(p) => p,
                None => break,
            };
            if !self.types_at_fat_arrow() {
                self.error_here(format!(
                    "erwartet '=>' nach dem muster, gefunden '{}'",
                    self.kind().text()
                ));
                self.recovering = false;
                break;
            }
            self.bump();
            self.bump();
            if !self.at(&TokKind::LBrace) {
                self.error_here(format!(
                    "erwartet '{{' nach '=>' (der rumpf eines falls ist ein block), gefunden '{}'",
                    self.kind().text()
                ));
                self.recovering = false;
                break;
            }
            let body = self.block("am anfang eines match-falls");
            self.recovering = false;
            let span = Parser::join(pat.span(), body.span);
            arms.push(Arm { pat, body, span });
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.close(TokKind::RBrace, "am ende von 'match'");
        self.recovering = false;
        let span = Parser::join(start, end);
        if arms.is_empty() {
            self.dg.error(span, "'match' braucht mindestens einen fall");
            return Stmt::Error(span);
        }
        let idx = REG.with(|r| {
            let mut reg = r.borrow_mut();
            reg.matches.push(MatchInfo { subject, arms, span });
            reg.matches.len() - 1
        });
        let name = format!("{}{}", MATCH_PREFIX, idx);
        let e = self.mk(span, ExprKind::Call(name, Vec::new(), span));
        Stmt::Expr(e)
    }

    fn types_pattern(&mut self, depth: u32) -> Option<Pattern> {
        if depth > 32 {
            self.error_here("muster ist zu tief verschachtelt (mehr als 32 ebenen)");
            self.recovering = false;
            return None;
        }
        let sp = self.span();
        match self.kind().clone() {
            TokKind::KwTrue => {
                self.bump();
                Some(Pattern::Bool(true, sp))
            }
            TokKind::KwFalse => {
                self.bump();
                Some(Pattern::Bool(false, sp))
            }
            TokKind::Int(_) | TokKind::Minus => {
                let lo = self.types_pattern_int()?;
                if self.types_at_dotdot() {
                    self.bump();
                    let inclusive = self.eat(&TokKind::Assign);
                    let hisp = self.span();
                    let hi = self.types_pattern_int()?;
                    let span = Parser::join(sp, hisp);
                    if (inclusive && hi < lo) || (!inclusive && hi <= lo) {
                        self.dg
                            .error(span, "der bereich im muster ist leer (obere grenze zu klein)");
                        return None;
                    }
                    return Some(Pattern::Range { lo, hi, inclusive, span });
                }
                Some(Pattern::Int(lo, sp))
            }
            TokKind::Ident(name) => {
                if name == "_" {
                    self.bump();
                    return Some(Pattern::Wild(sp));
                }
                self.bump();
                if self.types_at_colon2(0) {
                    self.types_eat_colon2();
                    let (vname, vspan) = self.ident("nach '::' im muster")?;
                    let mut subs = Vec::new();
                    let mut end = vspan;
                    if self.eat(&TokKind::LParen) {
                        loop {
                            if self.at(&TokKind::RParen) || self.at_eof() {
                                break;
                            }
                            let before = self.pos;
                            let p = self.types_pattern(depth + 1)?;
                            subs.push(p);
                            if !self.eat(&TokKind::Comma) {
                                break;
                            }
                            if self.pos == before {
                                self.bump();
                            }
                        }
                        end = self.span();
                        self.close(TokKind::RParen, "nach den untermustern");
                    }
                    return Some(Pattern::Variant {
                        ename: Some(name),
                        vname,
                        subs,
                        span: Parser::join(sp, end),
                    });
                }
                Some(Pattern::Bind(name, sp))
            }
            other => {
                self.error_here(format!("erwartet ein muster, gefunden '{}'", other.text()));
                self.recovering = false;
                None
            }
        }
    }

    fn types_pattern_int(&mut self) -> Option<i128> {
        let neg = self.eat(&TokKind::Minus);
        match self.kind().clone() {
            TokKind::Int(v) => {
                self.bump();
                Some(if neg { -v } else { v })
            }
            other => {
                self.error_here(format!(
                    "erwartet eine ganzzahl im muster, gefunden '{}'",
                    other.text()
                ));
                self.recovering = false;
                None
            }
        }
    }
}

/// `// HOOK types` in `parser.rs::program` — Aufzaehlungen und generische
/// Vorlagen auf oberster Ebene.
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    if matches!(p.kind(), TokKind::KwEnum) {
        p.types_enum_decl();
        return true;
    }
    crate::sema_generic::hook_item(p)
}

/// `// HOOK types` in `parser.rs::stmt_inner` — `match`-Anweisung.
pub(crate) fn hook_stmt(p: &mut Parser) -> Option<Stmt> {
    if matches!(p.kind(), TokKind::KwMatch) {
        return Some(p.types_match_stmt());
    }
    None
}

/// `// HOOK types` in `parser.rs::primary` — `Enum::Variante(..)`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    if let TokKind::Ident(name) = p.kind().clone() {
        if p.types_at_colon2(1) && matches!(p.tk(3), TokKind::Ident(_)) {
            let sp = p.bump();
            p.types_eat_colon2();
            let (vname, vspan) = p.ident("nach '::'")?;
            let full = format!("{}::{}", name, vname);
            let mut args = Vec::new();
            let mut end = vspan;
            if p.at(&TokKind::LParen) {
                p.bump();
                let (a, e) = p.call_args("nach den nutzdaten der variante");
                args = a;
                end = e;
            }
            let span = Parser::join(sp, end);
            return Some(p.mk(span, ExprKind::Call(full, args, span)));
        }
    }
    crate::sema_generic::hook_primary(p)
}

// ---------------------------------------------------- Anmeldung im Typkontext

/// `// HOOK types` in `sema::run` (vor `collect_structs`): meldet die Namen
/// aller Aufzaehlungen an, damit Structs und Funktionen sie benennen koennen.
pub(crate) fn declare_enums(ck: &mut Checker) {
    let n = enum_count();
    for i in 0..n {
        let def = match enum_at(i) {
            Some(d) => d,
            None => continue,
        };
        if ck.tcx.lookup(&def.name).is_some() {
            ck.dg
                .error(def.span, format!("typ '{}' ist bereits deklariert", def.name));
            continue;
        }
        let idx = ck.tcx.declare(&def.name);
        REG.with(|r| {
            let mut reg = r.borrow_mut();
            if let Some(d) = reg.enums.get_mut(i) {
                d.struct_idx = idx;
            }
            reg.by_struct.insert(idx, i);
        });
    }
}

/// `// HOOK types` in `sema::run` (nach `collect_structs`): berechnet das
/// Layout jeder Aufzaehlung und traegt es als Struct-Layout ein.
pub(crate) fn layout_enums(ck: &mut Checker, prog: &crate::ast::Program) {
    if enum_count() == 0 {
        return;
    }
    // Aufzaehlungen duerfen (noch) nicht dem Wert nach in einem Struct liegen:
    // die Struct-Layouts stehen zu diesem Zeitpunkt bereits fest.
    for s in &prog.structs {
        for (fname, te, span) in &s.fields {
            if let Some(n) = value_named(te) {
                if is_enum_name(&n) {
                    ck.dg.error_note(
                        *span,
                        format!(
                            "feld '{}' hat den aufzaehlungstyp '{}' — das wird in dieser stufe nicht unterstuetzt",
                            fname, n
                        ),
                        "benutze einen zeiger ('*mut T') auf die aufzaehlung",
                    );
                }
            }
        }
    }

    // Reihenfolge nach Abhaengigkeit (eine Aufzaehlung kann eine andere dem
    // Wert nach enthalten). Zyklen sind ein Fehler.
    let n = enum_count();
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut defs: Vec<EnumDef> = Vec::with_capacity(n);
    for i in 0..n {
        let d = match enum_at(i) {
            Some(d) => d,
            None => continue,
        };
        for v in &d.variants {
            for te in &v.field_tys {
                if let Some(name) = value_named(te) {
                    if let Some(j) = REG.with(|r| r.borrow().by_name.get(&name).copied()) {
                        deps[i].push(j);
                    }
                }
            }
        }
        defs.push(d);
    }
    let mut state = vec![0u8; n];
    let mut order: Vec<usize> = Vec::new();
    let mut bad: Vec<usize> = Vec::new();
    for i in 0..n {
        toposort(i, &deps, &mut state, &mut order, &mut bad);
    }
    for i in &bad {
        if let Some(d) = defs.get(*i) {
            ck.dg.error_note(
                d.span,
                format!("aufzaehlung '{}' enthaelt sich selbst (direkt oder indirekt)", d.name),
                "benutze an der stelle einen zeiger, z. B. '*mut T'",
            );
        }
    }

    for i in order {
        if bad.contains(&i) {
            continue;
        }
        let mut d = match defs.get(i).cloned() {
            Some(d) => d,
            None => continue,
        };
        if d.struct_idx == usize::MAX {
            continue;
        }
        // 1. Nutzdatentypen aufloesen, Ausrichtung bestimmen
        let mut payload_align: u64 = 1;
        let mut resolved: Vec<Vec<Type>> = Vec::new();
        for v in &d.variants {
            let mut tys = Vec::new();
            for te in &v.field_tys {
                let t = ck.resolve_ty(te);
                if matches!(t, Type::Void) {
                    ck.dg.error(te.span(), "nutzdaten einer variante koennen nicht den typ '()' haben");
                }
                let a = ck.tcx.align_of(&t).max(1);
                if a > payload_align {
                    payload_align = a;
                }
                tys.push(t);
            }
            resolved.push(tys);
        }
        let payload_off = round_up(4, payload_align);
        // 2. Offsets je Variante (Varianten ueberlagern sich)
        let mut max_end = payload_off;
        let mut fields: Vec<Field> = vec![Field {
            name: "__tag".to_string(),
            ty: Type::U32,
            offset: 0,
        }];
        for (vi, tys) in resolved.iter().enumerate() {
            let mut off = payload_off;
            let mut offsets = Vec::new();
            for (fi, t) in tys.iter().enumerate() {
                let a = ck.tcx.align_of(t).max(1);
                let sz = ck.tcx.size_of(t);
                off = round_up(off, a);
                offsets.push(off);
                fields.push(Field {
                    name: format!("__v{}_{}", vi, fi),
                    ty: t.clone(),
                    offset: off,
                });
                off += sz;
            }
            if off > max_end {
                max_end = off;
            }
            if let Some(v) = d.variants.get_mut(vi) {
                v.fields = tys.clone();
                v.offsets = offsets;
            }
        }
        let align = payload_align.max(4);
        let size = round_up(max_end, align).max(align);
        d.size = size;
        d.align = align;
        if let Some(sd) = ck.tcx.structs.get_mut(d.struct_idx) {
            *sd = StructDef {
                name: d.name.clone(),
                fields,
                size,
                align,
            };
        }
        REG.with(|r| {
            let mut reg = r.borrow_mut();
            if let Some(slot) = reg.enums.get_mut(i) {
                *slot = d.clone();
            }
        });
    }
}

/// Name eines Typs, sofern er DEM WERT NACH enthalten ist (Zeiger nicht).
fn value_named(te: &TypeExpr) -> Option<String> {
    match te {
        TypeExpr::Named(n, _) => Some(n.clone()),
        TypeExpr::Array { elem, .. } => value_named(elem),
        TypeExpr::Ptr { .. } => None,
    }
}

fn toposort(i: usize, deps: &[Vec<usize>], state: &mut Vec<u8>, order: &mut Vec<usize>, bad: &mut Vec<usize>) {
    match state.get(i) {
        Some(2) | None => return,
        Some(1) => {
            if !bad.contains(&i) {
                bad.push(i);
            }
            return;
        }
        _ => {}
    }
    state[i] = 1;
    for j in deps.get(i).map(|v| v.as_slice()).unwrap_or(&[]) {
        toposort(*j, deps, state, order, bad);
        if bad.contains(j) && !bad.contains(&i) {
            bad.push(i);
        }
    }
    state[i] = 2;
    order.push(i);
}

// ------------------------------------------------------------- Typpruefung

/// `// HOOK types` in `sema::stmt_returns`: kehrt ein `match` auf jedem Pfad
/// zurueck? Der Musterabgleich ist zu diesem Zeitpunkt geprueft, also
/// vollstaendig — es genuegt, dass jeder Fall zurueckkehrt.
pub(crate) fn match_returns(e: &Expr) -> bool {
    let idx = match &e.kind {
        ExprKind::Call(name, _, _) => match match_index_of(name) {
            Some(i) => i,
            None => return false,
        },
        _ => return false,
    };
    let mi = match match_info(idx) {
        Some(m) => m,
        None => return false,
    };
    !mi.arms.is_empty() && mi.arms.iter().all(|a| block_returns(&a.body))
}

fn block_returns(b: &Block) -> bool {
    b.stmts.iter().any(stmt_returns)
}

fn stmt_returns(s: &Stmt) -> bool {
    match s {
        Stmt::Return { .. } => true,
        Stmt::Block(b) => block_returns(b),
        Stmt::If { then, els, .. } => match els {
            Some(e) => block_returns(then) && stmt_returns(e),
            None => false,
        },
        Stmt::Expr(e) => match_returns(e),
        _ => false,
    }
}

/// `// HOOK types` in `sema::call`: faengt `Enum::Variante(..)` und `match` ab.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if let Some(idx) = match_index_of(name) {
        check_match(ck, idx, espan);
        return Some(Type::Void);
    }
    if let Some((ename, vname)) = name.split_once("::") {
        return Some(check_ctor(ck, ename, vname, args, nspan, espan));
    }
    None
}

fn check_ctor(
    ck: &mut Checker,
    ename: &str,
    vname: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Type {
    let def = match enum_by_name(ename) {
        Some(d) => d,
        None => {
            for a in args {
                ck.type_out_expr(a);
            }
            ck.dg
                .error(nspan, format!("unbekannte aufzaehlung '{}'", ename));
            return Type::Error;
        }
    };
    let v = match def.variant(vname) {
        Some(v) => v.clone(),
        None => {
            for a in args {
                ck.type_out_expr(a);
            }
            ck.dg.error_note(
                nspan,
                format!("aufzaehlung '{}' hat keine variante '{}'", ename, vname),
                format!("bekannt sind: {}", variant_list(&def)),
            );
            return Type::Error;
        }
    };
    if args.len() != v.fields.len() {
        ck.dg.error(
            espan,
            format!(
                "variante '{}::{}' erwartet {} nutzdatenwert(e), gefunden {}",
                ename,
                vname,
                v.fields.len(),
                args.len()
            ),
        );
    }
    for (i, a) in args.iter().enumerate() {
        match v.fields.get(i) {
            Some(want) => {
                let got = ck.expr(a, Some(want));
                if !assignable(&got, want) {
                    ck.dg.error(
                        a.span,
                        format!(
                            "nutzdatenwert {} von '{}::{}' hat typ {}, erwartet {}",
                            i + 1,
                            ename,
                            vname,
                            ck.tcx.name_of(&got),
                            ck.tcx.name_of(want)
                        ),
                    );
                }
            }
            None => ck.type_out_expr(a),
        }
    }
    Type::Struct(def.struct_idx)
}

fn variant_list(def: &EnumDef) -> String {
    def.variants
        .iter()
        .map(|v| v.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

fn assignable(got: &Type, want: &Type) -> bool {
    if got.is_error() || want.is_error() {
        return true;
    }
    match (got, want) {
        (Type::Ptr { inner: x, .. }, Type::Ptr { inner: y, .. }) => assignable(x, y),
        _ => got == want,
    }
}

fn check_match(ck: &mut Checker, idx: usize, espan: Span) {
    let mi = match match_info(idx) {
        Some(m) => m,
        None => {
            ck.dg
                .error(espan, "interner fehler: unbekannter musterabgleich");
            return;
        }
    };
    let sty = ck.expr(&mi.subject, None);
    let subject = classify_subject(ck, &sty, mi.subject.span);

    // 1. Muster pruefen, Bindungen anlegen, Rumpf pruefen
    for arm in &mi.arms {
        ck.scopes.push(HashMap::new());
        check_pattern(ck, &arm.pat, &subject_type(&subject, &sty), &subject, true);
        ck.check_block(&arm.body, true);
        ck.scopes.pop();
    }

    // 2. Erreichbarkeit: nach einem immer zutreffenden Fall kommt nichts mehr
    let mut catchall: Option<Span> = None;
    for arm in &mi.arms {
        if let Some(prev) = catchall {
            ck.dg.error_note(
                arm.span,
                "dieser fall ist unerreichbar",
                format!(
                    "ein frueherer fall in zeile {} trifft immer zu",
                    prev.line
                ),
            );
            break;
        }
        if arm.pat.is_irrefutable() {
            catchall = Some(arm.pat.span());
        }
    }

    // 3. Vollstaendigkeit — FEHLER, kein Warnhinweis (SPEC §6.3)
    if let Err(d) = check_exhaustive(&subject, &mi.arms, mi.span) {
        match d.note {
            Some(note) => ck.dg.error_note(d.span, d.msg, note),
            None => ck.dg.error(d.span, d.msg),
        }
    }
}

fn subject_type(s: &Subject, fallback: &Type) -> Type {
    match s {
        Subject::Enum(d) => Type::Struct(d.struct_idx),
        Subject::Bool => Type::Bool,
        Subject::Int(t) => t.clone(),
        Subject::Bad => fallback.clone(),
    }
}

fn classify_subject(ck: &mut Checker, ty: &Type, span: Span) -> Subject {
    if ty.is_error() {
        return Subject::Bad;
    }
    if let Type::Struct(i) = ty {
        if let Some(d) = enum_by_struct(*i) {
            return Subject::Enum(d);
        }
    }
    if *ty == Type::Bool {
        return Subject::Bool;
    }
    if ty.is_concrete_int() {
        return Subject::Int(ty.clone());
    }
    if *ty == Type::UntypedInt {
        ck.dg.error_note(
            span,
            "typ des ganzzahlausdrucks in 'match' ist nicht ableitbar",
            "schreibe z. B. 'x as i32'",
        );
        return Subject::Bad;
    }
    ck.dg.error(
        span,
        format!(
            "'match' arbeitet auf aufzaehlungen, ganzzahlen und bool, nicht auf {}",
            ck.tcx.name_of(ty)
        ),
    );
    Subject::Bad
}

/// Prueft ein Muster gegen den erwarteten Typ und legt seine Bindungen an.
fn check_pattern(ck: &mut Checker, pat: &Pattern, ty: &Type, subject: &Subject, top: bool) {
    match pat {
        Pattern::Wild(_) => {}
        Pattern::Bind(name, span) => {
            ck.declare_var(name, ty.clone(), false, *span);
        }
        Pattern::Bool(_, span) => {
            if !matches!(ty, Type::Bool | Type::Error) {
                ck.dg.error(
                    *span,
                    format!(
                        "muster 'true'/'false' passt nicht zum typ {}",
                        ck.tcx.name_of(ty)
                    ),
                );
            }
        }
        Pattern::Int(v, span) => {
            if !ty.is_concrete_int() && !ty.is_error() {
                ck.dg.error(
                    *span,
                    format!("zahlenmuster passt nicht zum typ {}", ck.tcx.name_of(ty)),
                );
            } else if !fits(*v, ty) {
                ck.dg.error(
                    *span,
                    format!("zahl {} passt nicht in den typ {}", v, ck.tcx.name_of(ty)),
                );
            }
        }
        Pattern::Range { lo, hi, span, .. } => {
            if !ty.is_concrete_int() && !ty.is_error() {
                ck.dg.error(
                    *span,
                    format!("bereichsmuster passt nicht zum typ {}", ck.tcx.name_of(ty)),
                );
            } else if !fits(*lo, ty) || !fits(*hi, ty) {
                ck.dg.error(
                    *span,
                    format!("die bereichsgrenzen passen nicht in den typ {}", ck.tcx.name_of(ty)),
                );
            }
        }
        Pattern::Variant { ename, vname, subs, span } => {
            // Aufzaehlung des Musters bestimmen
            let def = match ename {
                Some(n) => match enum_by_name(n) {
                    Some(d) => Some(d),
                    None => {
                        ck.dg.error(*span, format!("unbekannte aufzaehlung '{}'", n));
                        None
                    }
                },
                None => match ty {
                    Type::Struct(i) => enum_by_struct(*i),
                    _ => None,
                },
            };
            let def = match def {
                Some(d) => d,
                None => return,
            };
            let want = Type::Struct(def.struct_idx);
            if !ty.is_error() && *ty != want {
                ck.dg.error(
                    *span,
                    format!(
                        "muster der aufzaehlung '{}' passt nicht zum typ {}",
                        def.name,
                        ck.tcx.name_of(ty)
                    ),
                );
                return;
            }
            if top {
                if let Subject::Enum(sd) = subject {
                    if sd.name != def.name {
                        ck.dg.error(
                            *span,
                            format!(
                                "muster der aufzaehlung '{}' passt nicht zum typ '{}'",
                                def.name, sd.name
                            ),
                        );
                        return;
                    }
                }
            }
            let v = match def.variant(vname) {
                Some(v) => v.clone(),
                None => {
                    ck.dg.error_note(
                        *span,
                        format!("aufzaehlung '{}' hat keine variante '{}'", def.name, vname),
                        format!("bekannt sind: {}", variant_list(&def)),
                    );
                    return;
                }
            };
            if subs.len() != v.fields.len() {
                ck.dg.error(
                    *span,
                    format!(
                        "muster '{}::{}' erwartet {} untermuster, gefunden {}",
                        def.name,
                        vname,
                        v.fields.len(),
                        subs.len()
                    ),
                );
            }
            for (i, sp) in subs.iter().enumerate() {
                if let Some(ft) = v.fields.get(i) {
                    check_pattern(ck, sp, ft, subject, false);
                }
            }
        }
    }
}

fn fits(v: i128, t: &Type) -> bool {
    let (lo, hi): (i128, i128) = match t {
        Type::I8 => (-128, 127),
        Type::I16 => (-32768, 32767),
        Type::I32 => (-2147483648, 2147483647),
        Type::I64 | Type::Isize => (i64::MIN as i128, i64::MAX as i128),
        Type::U8 => (0, 255),
        Type::U16 => (0, 65535),
        Type::U32 => (0, 4294967295),
        Type::U64 | Type::Usize => (0, u64::MAX as i128),
        _ => return true,
    };
    v >= lo && v <= hi
}

/// **Vollstaendigkeitspruefung zur Uebersetzungszeit** (SPEC §6.3).
///
/// Liefert `Err(Diag)` mit Zeile/Spalte und dem Namen der fehlenden Variante,
/// wenn der Musterabgleich einen Fall nicht abdeckt. Das ist ein Fehler, kein
/// Warnhinweis — der Aufrufer meldet ihn ueber `Diags`.
pub fn check_exhaustive(subject: &Subject, arms: &[Arm], span: Span) -> Result<(), Diag> {
    let hat_catchall = arms.iter().any(|a| a.pat.is_irrefutable());
    match subject {
        Subject::Bad => Ok(()),
        Subject::Enum(def) => {
            if hat_catchall {
                return Ok(());
            }
            let mut fehlend: Vec<String> = Vec::new();
            for v in &def.variants {
                let abgedeckt = arms.iter().any(|a| match &a.pat {
                    Pattern::Variant { vname, subs, .. } => {
                        *vname == v.name && subs.iter().all(|s| s.is_irrefutable())
                    }
                    _ => false,
                });
                if !abgedeckt {
                    fehlend.push(format!("{}::{}", def.name, v.name));
                }
            }
            if fehlend.is_empty() {
                return Ok(());
            }
            let liste = fehlend.join(", ");
            Err(Diag {
                msg: format!(
                    "'match' ist nicht vollstaendig: {} nicht abgedeckt",
                    if fehlend.len() == 1 {
                        format!("die variante {} ist", liste)
                    } else {
                        format!("die varianten {} sind", liste)
                    }
                ),
                span,
                label: "hier".to_string(),
                note: Some(format!(
                    "ergaenze einen fall '{} => {{ }}' oder '_ => {{ }}'",
                    fehlend[0]
                )),
            })
        }
        Subject::Bool => {
            if hat_catchall {
                return Ok(());
            }
            let hat = |b: bool| {
                arms.iter()
                    .any(|a| matches!(&a.pat, Pattern::Bool(x, _) if *x == b))
            };
            let mut fehlend = Vec::new();
            if !hat(true) {
                fehlend.push("true");
            }
            if !hat(false) {
                fehlend.push("false");
            }
            if fehlend.is_empty() {
                return Ok(());
            }
            Err(Diag {
                msg: format!(
                    "'match' ist nicht vollstaendig: der fall {} fehlt",
                    fehlend.join(" und ")
                ),
                span,
                label: "hier".to_string(),
                note: Some("ergaenze den fehlenden fall oder '_ => { }'".to_string()),
            })
        }
        Subject::Int(t) => {
            if hat_catchall {
                return Ok(());
            }
            Err(Diag {
                msg: format!(
                    "'match' ueber {} ist nicht vollstaendig: es fehlt ein fall fuer alle uebrigen werte",
                    type_name(t)
                ),
                span,
                label: "hier".to_string(),
                note: Some("ergaenze '_ => { }'".to_string()),
            })
        }
    }
}

fn type_name(t: &Type) -> &'static str {
    match t {
        Type::I8 => "i8",
        Type::I16 => "i16",
        Type::I32 => "i32",
        Type::I64 => "i64",
        Type::U8 => "u8",
        Type::U16 => "u16",
        Type::U32 => "u32",
        Type::U64 => "u64",
        Type::Usize => "usize",
        Type::Isize => "isize",
        _ => "ganzzahlen",
    }
}

#[cfg(test)]
mod tests {
    use crate::diag::Diags;

    fn uebersetze(src: &str) -> (String, bool) {
        let mut dg = Diags::new("test.fi", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        crate::mono::expand(&mut prog, &mut dg);
        if dg.has_errors() {
            return (dg.render(), false);
        }
        let info = match crate::sema::check(&prog, &mut dg) {
            Some(i) => i,
            None => return (dg.render(), false),
        };
        if dg.has_errors() {
            return (dg.render(), false);
        }
        match crate::lower::lower(&prog, &info, &mut dg) {
            Some(_) => (dg.render(), !dg.has_errors()),
            None => (dg.render(), false),
        }
    }

    #[test]
    fn fehlende_variante_ist_ein_fehler_mit_namen() {
        let src = "\
enum T { A, B(i32), C }
fn main() -> i32 {
    let t = T::B(3 as i32)
    match t {
        T::A => { }
        T::B(x) => { }
    }
    return 0 as i32
}
";
        let (out, ok) = uebersetze(src);
        assert!(!ok, "unvollstaendiges match muss ein fehler sein:\n{}", out);
        assert!(out.contains("nicht vollstaendig"), "{}", out);
        assert!(out.contains("T::C"), "{}", out);
    }

    #[test]
    fn vollstaendiges_match_uebersetzt() {
        let src = "\
enum T { A, B(i32) }
fn main() -> i32 {
    var r: i32 = 0 as i32
    let t = T::B(7 as i32)
    match t {
        T::A => { r = 1 as i32 }
        T::B(x) => { r = x }
    }
    return r
}
";
        let (out, ok) = uebersetze(src);
        assert!(ok, "{}", out);
    }

    #[test]
    fn ganzzahl_match_braucht_einen_auffangfall() {
        let src = "\
fn main() -> i32 {
    let n: i32 = 3 as i32
    match n {
        0 => { }
        1 => { }
    }
    return 0 as i32
}
";
        let (out, ok) = uebersetze(src);
        assert!(!ok, "{}", out);
        assert!(out.contains("nicht vollstaendig"), "{}", out);
    }

    #[test]
    fn layout_tag_und_nutzdaten() {
        let src = "\
enum T { A, B(i64) }
fn main() -> i32 { return 0 as i32 }
";
        let mut dg = Diags::new("test.fi", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let prog = crate::parser::parse(&toks, &mut dg);
        let info = crate::sema::check(&prog, &mut dg).expect("typpruefung");
        let idx = info.tcx.lookup("T").expect("aufzaehlung T");
        let sd = &info.tcx.structs[idx];
        assert_eq!(sd.field("__tag").expect("tag").offset, 0);
        assert_eq!(sd.field("__v1_0").expect("nutzdaten").offset, 8);
        assert_eq!(sd.size, 16);
        assert_eq!(sd.align, 8);
    }
}
