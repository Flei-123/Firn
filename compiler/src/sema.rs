//! Typpruefer (SPEC §12).
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn check(prog: &ast::Program, dg: &mut Diags) -> Option<TypeInfo>`
//! Bei Erfolg gilt die ZUSICHERUNG: fuer JEDE vergebene `ExprId` steht in
//! `TypeInfo::expr_types` ein konkreter Typ (nie `Type::UntypedInt`, nie
//! `Type::Error`). Darauf verlaesst sich `lower.rs`.
//!
//! Aufbau:
//!  1. Structs anlegen, Feldtypen aufloesen, Rekursion (direkt/indirekt)
//!     erkennen, Layout in topologischer Reihenfolge berechnen.
//!  2. Funktionssignaturen sammeln (nur skalare Parameter/Rueckgabe, SPEC §12.1).
//!  3. `const`-Deklarationen pruefen und zur Uebersetzungszeit auswerten.
//!  4. Rumpfe pruefen: bidirektional (Kontexttyp als Hinweis fuer typlose
//!     Ganzzahlliterale), keine impliziten Umwandlungen, Erreichbarkeits-
//!     analyse fuer `return`.
//!
//! Es werden moeglichst viele Fehler gesammelt: nach einem Fehler laeuft die
//! Pruefung mit `Type::Error` weiter, der mit allem vertraeglich ist.

use std::collections::{HashMap, HashSet};

use crate::ast::{
    BinOp, Block, Expr, ExprId, ExprKind, FnDecl, Program, Stmt, TypeExpr, UnOp,
};
use crate::diag::{Diags, Span};
use crate::types::{Type, TypeCtx};

#[derive(Clone, Debug)]
pub struct FnSig {
    pub params: Vec<Type>,
    pub ret: Type,
}

#[derive(Clone, Debug, Default)]
pub struct TypeInfo {
    /// Struct-Tabelle inklusive berechnetem Layout.
    pub tcx: TypeCtx,
    /// Typ jedes Ausdrucks, indiziert mit `ExprId`.
    pub expr_types: Vec<Type>,
    /// Ausgewertete `const`-Deklarationen: Name -> (Typ, Wert).
    pub consts: HashMap<String, (Type, i128)>,
    /// Signaturen aller Funktionen.
    pub fns: HashMap<String, FnSig>,
}

impl TypeInfo {
    pub fn expr_ty(&self, id: crate::ast::ExprId) -> &Type {
        self.expr_types.get(id as usize).unwrap_or(&Type::Error)
    }
}

/// Warum ein lvalue nicht beschreibbar ist.
#[derive(Clone, Debug)]
enum Mutability {
    Mutable,
    Fixed(String),
}

#[derive(Clone, Debug)]
pub(crate) struct VarInfo {
    pub(crate) ty: Type,
    pub(crate) mutable: bool,
}

/// Hoechste Verschachtelungstiefe von Ausdruecken (Schutz vor Stapelueberlauf).
const MAX_DEPTH: u32 = 200;

pub(crate) struct Checker<'a> {
    pub(crate) dg: &'a mut Diags,
    pub(crate) tcx: TypeCtx,
    pub(crate) fns: HashMap<String, FnSig>,
    pub(crate) consts: HashMap<String, (Type, i128)>,
    pub(crate) expr_types: Vec<Type>,
    pub(crate) scopes: Vec<HashMap<String, VarInfo>>,
    pub(crate) ret: Type,
    pub(crate) depth: u32,
    /// Funktionen mit `#[must_consume]` — ihr Ergebnis darf nicht als
    /// Anweisung verworfen werden (attrs.rs).
    pub(crate) must_consume_fns: HashSet<String>,
}

pub fn check(prog: &Program, dg: &mut Diags) -> Option<TypeInfo> {
    let mut ck = Checker {
        dg,
        tcx: TypeCtx::new(),
        fns: HashMap::new(),
        consts: HashMap::new(),
        expr_types: vec![Type::Error; prog.expr_count as usize],
        scopes: Vec::new(),
        ret: Type::Void,
        depth: 0,
        must_consume_fns: HashSet::new(),
    };
    ck.run(prog);
    if ck.dg.has_errors() {
        return None;
    }
    // Vom Parser vergebene, aber verworfene ExprIds (Fehlerwiederherstellung)
    // bekommen einen konkreten Fuellwert, damit die Zusicherung haelt. Sie sind
    // in keinem AST-Knoten erreichbar, das Lowering sieht sie nie.
    for t in ck.expr_types.iter_mut() {
        if !matches!(t, Type::Error) {
            continue;
        }
        *t = Type::I64;
    }
    Some(TypeInfo {
        tcx: ck.tcx,
        expr_types: ck.expr_types,
        consts: ck.consts,
        fns: ck.fns,
    })
}

impl<'a> Checker<'a> {
    fn run(&mut self, prog: &Program) {
        self.check_profile(prog);
        // HOOK types: Aufzaehlungsnamen anmelden (sema_match.rs)
        crate::sema_match::declare_enums(self);
        // HOOK fehlerunionen: Fehlermengen anmelden (errors.rs)
        crate::errors::declare_error_sets(self);
        // HOOK gc: `gc class` als Struct mit Praefixlayout anmelden, Typkennung
        // und Ahnenkette berechnen (gc.rs, SPEC 3.5.1)
        crate::gc::declare_classes(self);
        self.add_items_inner(prog, true);
        // HOOK nogc: `#[no_gc]` transitiv pruefen (nogc.rs, SPEC 3.5.4).
        // Laeuft NACH der Typpruefung, weil Regel 3 (Schreiben in ein
        // Gc[T]-Feld) die Typtabelle braucht.
        crate::nogc::hook_check(self, prog);
        // Ganzprogramm-Pruefung: laeuft genau einmal, nicht je Nachtrag.
        self.check_main(prog);
    }

    /// **Wiedereintritt in die Pruefphasen** (DESIGNZIELE.md §7, Fundamentpunkt
    /// aus §10.4).
    ///
    /// Prueft ZUSAETZLICHE Deklarationen mit dem bereits aufgebauten Zustand —
    /// dieselbe Namenstabelle, dieselbe Typtabelle, dieselben Diagnosen. Damit
    /// ist die Frage „kann der Compiler eine gerade erst entstandene Funktion
    /// noch pruefen?" mit **ja** beantwortet.
    ///
    /// Gebraucht wird das von `comptime`/`emit` (SPEC §6.4): dort entstehen
    /// Elemente *waehrend* der Uebersetzung — Web-IDL-Bindungen,
    /// CSS-Eigenschaftstabellen, Unicode-Tabellen. Ein Typpruefer, der als
    /// einmaliger Durchlauf ueber einen festen AST gebaut ist, kann das
    /// nachtraeglich nicht mehr lernen; deshalb sitzt die Faehigkeit hier, bevor
    /// es einen Erzeuger dafuer gibt.
    ///
    /// **Ehrlicher Umfang:** Nachtraege duerfen Structs, Funktionen und
    /// Konstanten enthalten. Aufzaehlungen werden nur im ersten Durchlauf
    /// ausgelegt (`layout_enums`), weil ihre Anmeldung im Parser passiert;
    /// nachtraeglich erzeugte `enum`s kommen mit `comptime` selbst.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn add_items(&mut self, prog: &Program) {
        self.add_items_inner(prog, false);
    }

    fn add_items_inner(&mut self, prog: &Program, layout_enums: bool) {
        // Nachtraege bringen eigene Ausdrucks-Ids mit; die Tabelle waechst mit.
        if prog.expr_count as usize > self.expr_types.len() {
            self.expr_types.resize(prog.expr_count as usize, Type::Error);
        }
        self.collect_structs(prog);
        if layout_enums {
            // HOOK types: Aufzaehlungen auslegen (sema_match.rs)
            crate::sema_match::layout_enums(self, prog);
        }
        // HOOK gc: Feldlayout der gc-Klassen (gc.rs). Erst hier sind die
        // Structs und Aufzaehlungen des Programms bekannt — ein Structfeld in
        // einer gc-Klasse bekommt so die richtige Meldung.
        crate::gc::layout_classes(self);
        self.collect_fns(prog);
        // Attribute pruefen und anwenden (attrs.rs)
        self.check_attrs(prog);
        self.check_consts(prog);
        for f in &prog.funcs {
            self.check_fn(f);
        }
    }

    /// Wird hier ein Wert weggeworfen, der nicht weggeworfen werden darf?
    ///
    /// **Umfang in Stufe 0, ehrlich benannt:** Geprueft wird der Fall
    /// „Ergebnis eines Aufrufs wird als Anweisung verworfen". Die volle Form
    /// aus SPEC §3.3 — *der Wert muss an eine verbrauchende Funktion
    /// uebergeben werden* — braucht den Move-Pruefer und kommt mit ihm
    /// (ROADMAP Phase 2). Was hier geprueft wird, ist die Teilmenge, die ohne
    /// Move-Verfolgung entscheidbar ist.
    fn check_discard(&mut self, e: &Expr, t: &Type) {
        let name = match &e.kind {
            ExprKind::Call(n, _, _) => n.clone(),
            _ => return,
        };
        let grund = if self.must_consume_fns.contains(&name) {
            format!("'{}' ist mit #[must_consume] gekennzeichnet", name)
        } else if let Type::Struct(i) = t {
            match self.tcx.structs.get(*i) {
                Some(d) if d.must_consume => {
                    format!("der typ '{}' ist mit #[must_consume] gekennzeichnet", d.name)
                }
                _ => return,
            }
        } else {
            return;
        };
        self.dg.error_note(
            e.span,
            format!("das ergebnis darf nicht verworfen werden: {}", grund),
            "binde es an eine variable oder uebergib es weiter".to_string(),
        );
    }

    // ------------------------------------------------------------- Attribute

    /// Prueft alle Attribute gegen das Register in `attrs.rs` und wendet die
    /// an, die in Stufe 0 wirklich etwas tun.
    ///
    /// Drei Fehlerarten, alle mit Zeile und Spalte:
    ///  * **unbekannt** — mit Vorschlag, falls es ein Tippfehler ist
    ///  * **falsches Ziel** — z. B. `#[packed]` vor einer Funktion
    ///  * **bekannt, aber in Stufe 0 nicht umgesetzt** — ausdrueckliche
    ///    Ablehnung statt stillem Ignorieren. Ein uebergangenes
    ///    `#[constant_time]` waere der gefaehrlichste Fehler dieser Sprache.
    fn check_attrs(&mut self, prog: &Program) {
        for f in &prog.funcs {
            let attrs = f.attrs.clone();
            for a in &attrs {
                if self.check_one_attr(a, true) && a.name == "must_consume" {
                    self.must_consume_fns.insert(f.name.clone());
                }
            }
        }
        for sd in &prog.structs {
            let attrs = sd.attrs.clone();
            for a in &attrs {
                if self.check_one_attr(a, false) && a.name == "must_consume" {
                    if let Some(i) = self.tcx.lookup(&sd.name) {
                        if let Some(def) = self.tcx.structs.get_mut(i) {
                            def.must_consume = true;
                        }
                    }
                }
            }
        }
    }

    /// `true` = Attribut ist gueltig UND in Stufe 0 umgesetzt.
    fn check_one_attr(&mut self, a: &crate::ast::Attr, auf_funktion: bool) -> bool {
        let info = match crate::attrs::suche(&a.name) {
            Some(i) => i,
            None => {
                let msg = format!("unbekanntes attribut '{}'", a.name);
                match crate::attrs::vorschlag(&a.name) {
                    Some(v) => self.dg.error_note(
                        a.span,
                        msg,
                        format!("meintest du '{}'? '--list-attrs' zeigt alle", v),
                    ),
                    None => self.dg.error_note(
                        a.span,
                        msg,
                        "'--list-attrs' zeigt alle bekannten attribute".to_string(),
                    ),
                }
                return false;
            }
        };
        if !crate::attrs::passt(info, auf_funktion) {
            self.dg.error(
                a.span,
                format!(
                    "attribut '{}' gehoert nicht vor {}",
                    a.name,
                    if auf_funktion { "eine funktion" } else { "einen struct" }
                ),
            );
            return false;
        }
        if a.args.len() != info.args {
            self.dg.error(
                a.span,
                format!(
                    "attribut '{}' erwartet {} argument(e), gefunden {}",
                    a.name,
                    info.args,
                    a.args.len()
                ),
            );
            return false;
        }
        if !info.umgesetzt {
            self.dg.error_note(
                a.span,
                format!("attribut '{}' ist in Stufe 0 nicht umgesetzt", a.name),
                format!("geplant: {}", info.was),
            );
            return false;
        }
        true
    }

    // ---------------------------------------------------------------- Profil

    fn check_profile(&mut self, prog: &Program) {
        if let Some((name, span)) = &prog.profile {
            if name != "kernel" && name != "app" {
                self.dg.error_note(
                    *span,
                    format!("unbekanntes profil '{}'", name),
                    "erlaubt sind 'kernel' und 'app'",
                );
            }
        }
    }

    // --------------------------------------------------------------- Structs

    fn collect_structs(&mut self, prog: &Program) {
        // HOOK fehlerunionen: Layoutphase melden (errors.rs)
        crate::errors::hook_struct_phase(true);
        // 1. alle Namen anlegen (erlaubt gegenseitige Zeigerverweise)
        let mut idx_of: Vec<usize> = Vec::with_capacity(prog.structs.len());
        for s in &prog.structs {
            if self.tcx.lookup(&s.name).is_some() {
                self.dg
                    .error(s.span, format!("struct '{}' ist bereits deklariert", s.name));
                // Doppelte Deklaration: auf den ersten Eintrag zeigen lassen.
                idx_of.push(self.tcx.lookup(&s.name).unwrap_or(0));
                continue;
            }
            idx_of.push(self.tcx.declare(&s.name));
        }

        // 2. Feldtypen aufloesen
        let mut resolved: Vec<Vec<(String, Type)>> = Vec::with_capacity(prog.structs.len());
        for s in &prog.structs {
            let mut seen: HashSet<String> = HashSet::new();
            let mut fields: Vec<(String, Type)> = Vec::new();
            for (name, te, span) in &s.fields {
                let ty = self.resolve_ty(te);
                if !seen.insert(name.clone()) {
                    self.dg.error(
                        *span,
                        format!("feld '{}' ist in struct '{}' bereits deklariert", name, s.name),
                    );
                    continue;
                }
                if matches!(ty, Type::Void) {
                    self.dg
                        .error(te.span(), "ein feld kann nicht den typ '()' haben");
                    continue;
                }
                fields.push((name.clone(), ty));
            }
            resolved.push(fields);
        }

        // 3. Rekursion erkennen (Wertcontainment; Zeiger unterbrechen den Zyklus)
        let n = self.tcx.structs.len();
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, fields) in resolved.iter().enumerate() {
            let target = match idx_of.get(i) {
                Some(t) => *t,
                None => continue,
            };
            for (_, ty) in fields {
                collect_value_deps(ty, &mut deps[target]);
            }
        }
        let mut state = vec![0u8; n]; // 0 = neu, 1 = auf dem Pfad, 2 = fertig
        let mut order: Vec<usize> = Vec::new();
        let mut bad: HashSet<usize> = HashSet::new();
        for i in 0..n {
            find_cycles(i, &deps, &mut state, &mut order, &mut bad);
        }
        for (i, s) in prog.structs.iter().enumerate() {
            let target = match idx_of.get(i) {
                Some(t) => *t,
                None => continue,
            };
            if bad.contains(&target) {
                self.dg.error_note(
                    s.span,
                    format!("struct '{}' enthaelt sich selbst (direkt oder indirekt)", s.name),
                    "benutze an der Stelle einen zeiger, z. B. '*mut T'",
                );
            }
        }

        // 4. Layout in topologischer Reihenfolge berechnen
        let mut fields_by_idx: Vec<Option<Vec<(String, Type)>>> = vec![None; n];
        for (i, fields) in resolved.into_iter().enumerate() {
            if let Some(t) = idx_of.get(i) {
                if fields_by_idx[*t].is_none() {
                    fields_by_idx[*t] = Some(fields);
                }
            }
        }
        for idx in order {
            let fields = match fields_by_idx.get_mut(idx).and_then(|f| f.take()) {
                Some(f) => f,
                None => continue,
            };
            if bad.contains(&idx) {
                // Zyklische Structs bekommen kein Layout (Groesse 0), damit
                // size_of nicht endlos laeuft. Der Fehler ist schon gemeldet.
                self.tcx.set_fields(idx, Vec::new());
                continue;
            }
            self.tcx.set_fields(idx, fields);
        }
        // HOOK fehlerunionen: Layoutphase beendet (errors.rs)
        crate::errors::hook_struct_phase(false);
    }

    // ------------------------------------------------------------- Funktionen

    fn collect_fns(&mut self, prog: &Program) {
        for f in &prog.funcs {
            let mut params = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            for p in &f.params {
                let ty = self.resolve_ty(&p.ty);
                if !seen.insert(p.name.clone()) {
                    self.dg.error(
                        p.span,
                        format!("parameter '{}' ist bereits deklariert", p.name),
                    );
                }
                if matches!(ty, Type::Void) {
                    self.dg.error(p.ty.span(), "ein parameter kann nicht den typ '()' haben");
                    params.push(Type::Error);
                    continue;
                }
                params.push(ty);
            }
            let ret = match &f.ret {
                None => Type::Void,
                Some(te) => self.resolve_ty(te),
            };
            if self.fns.contains_key(&f.name) {
                self.dg.error(
                    f.span,
                    format!("funktion '{}' ist bereits deklariert", f.name),
                );
                continue;
            }
            self.fns.insert(f.name.clone(), FnSig { params, ret });
        }
    }

    fn check_main(&mut self, prog: &Program) {
        match self.fns.get("main") {
            None => {
                self.dg.error_note(
                    Span::none(),
                    "das programm hat keine funktion 'main'",
                    "erwartet wird 'fn main() -> i32'",
                );
            }
            Some(sig) => {
                let bad = !sig.params.is_empty() || sig.ret != Type::I32;
                if bad {
                    let span = prog
                        .funcs
                        .iter()
                        .find(|f| f.name == "main")
                        .map(|f| f.span)
                        .unwrap_or_else(Span::none);
                    self.dg.error_note(
                        span,
                        "'main' muss ohne parameter deklariert sein und 'i32' zurueckgeben",
                        "erwartet wird 'fn main() -> i32'",
                    );
                }
            }
        }
    }

    fn check_fn(&mut self, f: &FnDecl) {
        let sig = match self.fns.get(&f.name) {
            Some(s) => s.clone(),
            None => return, // doppelter Name, bereits gemeldet
        };
        self.ret = sig.ret.clone();
        self.scopes.clear();
        self.scopes.push(HashMap::new());
        for (p, ty) in f.params.iter().zip(sig.params.iter()) {
            self.declare_var(&p.name, ty.clone(), false, p.span);
        }
        self.check_block(&f.body, true);
        self.scopes.pop();
        if sig.ret != Type::Void && !sig.ret.is_error() && !block_returns(&f.body) {
            self.dg.error_note(
                f.span,
                format!(
                    "funktion '{}' erreicht das ende ohne 'return' (rueckgabetyp {})",
                    f.name,
                    self.tcx.name_of(&sig.ret)
                ),
                "jeder pfad muss mit 'return <wert>' enden",
            );
        }
    }

    // ------------------------------------------------------------- Konstanten

    fn check_consts(&mut self, prog: &Program) {
        for c in &prog.consts {
            let ty = self.resolve_ty(&c.ty);
            if !ty.is_error() && !(ty.is_concrete_int() || ty == Type::Bool) {
                self.dg.error(
                    c.ty.span(),
                    "'const' unterstuetzt in stufe 0 nur ganzzahl- und bool-typen",
                );
                self.type_out_expr(&c.value);
                continue;
            }
            let t = self.expr(&c.value, Some(&ty));
            if !assignable(&t, &ty) {
                self.dg.error(
                    c.value.span,
                    format!(
                        "konstante '{}' hat typ {}, der wert ist vom typ {}",
                        c.name,
                        self.tcx.name_of(&ty),
                        self.tcx.name_of(&t)
                    ),
                );
                continue;
            }
            if self.consts.contains_key(&c.name) {
                self.dg.error(
                    c.span,
                    format!("konstante '{}' ist bereits deklariert", c.name),
                );
                continue;
            }
            match self.eval_const(&c.value) {
                Ok(v) => {
                    self.consts.insert(c.name.clone(), (ty.clone(), wrap(v, &ty)));
                }
                Err((span, msg)) => {
                    self.dg.error(span, msg);
                    // Damit Folgeverwendungen keinen "unbekannter name"-Fehler geben.
                    self.consts.insert(c.name.clone(), (ty.clone(), 0));
                }
            }
        }
    }

    // -------------------------------------------------------------- Bereiche

    pub(crate) fn declare_var(&mut self, name: &str, ty: Type, mutable: bool, span: Span) {
        if let Some(top) = self.scopes.last() {
            if top.contains_key(name) {
                self.dg.error(
                    span,
                    format!("'{}' ist in diesem block bereits deklariert", name),
                );
            }
        }
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string(), VarInfo { ty, mutable });
        }
    }

    pub(crate) fn lookup_var(&self, name: &str) -> Option<&VarInfo> {
        for s in self.scopes.iter().rev() {
            if let Some(v) = s.get(name) {
                return Some(v);
            }
        }
        None
    }

    // ----------------------------------------------------------- Anweisungen

    pub(crate) fn check_block(&mut self, b: &Block, reuse_scope: bool) {
        if !reuse_scope {
            self.scopes.push(HashMap::new());
        }
        for s in &b.stmts {
            self.check_stmt(s);
        }
        if !reuse_scope {
            self.scopes.pop();
        }
    }

    fn check_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Error(_) => {}
            Stmt::Block(b) => self.check_block(b, false),
            Stmt::Expr(e) => {
                let t = self.expr(e, None);
                self.check_discard(e, &t);
            }
            Stmt::Let { name, mutable, ty, init, span } => {
                let declared = ty.as_ref().map(|te| self.resolve_ty(te));
                let t = match &declared {
                    // HOOK fehlerunionen: implizite Umwandlung (errors.rs)
                    Some(d) if crate::errors::hook_coerce(self, init, d) => d.clone(),
                    Some(d) => {
                        let got = self.expr(init, Some(d));
                        if !assignable(&got, d) {
                            self.dg.error(
                                init.span,
                                format!(
                                    "erwartet typ {}, gefunden {}",
                                    self.tcx.name_of(d),
                                    self.tcx.name_of(&got)
                                ),
                            );
                        }
                        d.clone()
                    }
                    None => {
                        let got = self.expr(init, None);
                        if matches!(got, Type::Void) {
                            self.dg.error(
                                init.span,
                                "der ausdruck liefert keinen wert und kann nicht gebunden werden",
                            );
                            Type::Error
                        } else {
                            got
                        }
                    }
                };
                self.declare_var(name, t, *mutable, *span);
            }
            Stmt::Assign { target, value, span } => {
                let (ty, mutability) = match self.lvalue(target) {
                    Some(x) => x,
                    None => {
                        self.expr(value, Some(&Type::I64));
                        return;
                    }
                };
                if let Mutability::Fixed(reason) = mutability {
                    self.dg.error_note(*span, reason, "benutze 'var' statt 'let'");
                }
                // HOOK fehlerunionen: implizite Umwandlung (errors.rs)
                if crate::errors::hook_coerce(self, value, &ty) {
                    return;
                }
                let got = self.expr(value, Some(&ty));
                if !assignable(&got, &ty) {
                    self.dg.error(
                        value.span,
                        format!(
                            "zuweisung erwartet typ {}, gefunden {}",
                            self.tcx.name_of(&ty),
                            self.tcx.name_of(&got)
                        ),
                    );
                }
            }
            Stmt::If { cond, then, els, .. } => {
                self.check_cond(cond, "if");
                self.check_block(then, false);
                if let Some(e) = els {
                    self.check_stmt(e);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.check_cond(cond, "while");
                self.check_block(body, false);
            }
            Stmt::Break(_) | Stmt::Continue(_) => {
                // Die Lage in einer Schleife prueft bereits der Parser.
            }
            Stmt::For { name, start, end, body, name_span, .. } => {
                let want = self.probe(start).or_else(|| self.probe(end));
                let st = self.expr(start, want.as_ref());
                let et = self.expr(end, want.as_ref().or(Some(&st)));
                let ty = if st.is_error() || et.is_error() {
                    Type::Error
                } else if !st.is_concrete_int() || !et.is_concrete_int() || st != et {
                    self.dg.error_note(
                        start.span,
                        format!(
                            "der bereich von 'for' braucht zwei werte desselben ganzzahltyps, gefunden {} und {}",
                            self.tcx.name_of(&st),
                            self.tcx.name_of(&et)
                        ),
                        "schreibe z. B. 'for i in 0 as usize..n'",
                    );
                    Type::Error
                } else {
                    st
                };
                self.scopes.push(HashMap::new());
                self.declare_var(name, ty, false, *name_span);
                self.check_block(body, true);
                self.scopes.pop();
            }
            Stmt::Return { value, span } => {
                let want = self.ret.clone();
                match value {
                    None => {
                        if want != Type::Void && !want.is_error() {
                            self.dg.error(
                                *span,
                                format!(
                                    "'return' ohne wert, erwartet wird ein wert vom typ {}",
                                    self.tcx.name_of(&want)
                                ),
                            );
                        }
                    }
                    Some(e) => {
                        if want == Type::Void {
                            self.expr(e, Some(&Type::I64));
                            self.dg.error(
                                e.span,
                                "diese funktion hat keinen rueckgabetyp, 'return' darf keinen wert haben",
                            );
                            return;
                        }
                        // HOOK fehlerunionen: implizite Umwandlung (errors.rs)
                        if crate::errors::hook_coerce(self, e, &want) {
                            return;
                        }
                        let got = self.expr(e, Some(&want));
                        if !assignable(&got, &want) {
                            self.dg.error(
                                e.span,
                                format!(
                                    "'return' erwartet typ {}, gefunden {}",
                                    self.tcx.name_of(&want),
                                    self.tcx.name_of(&got)
                                ),
                            );
                        }
                    }
                }
            }
        }
    }

    fn check_cond(&mut self, e: &Expr, kw: &str) {
        let t = self.expr(e, Some(&Type::Bool));
        if !t.is_error() && t != Type::Bool {
            self.dg.error_note(
                e.span,
                format!(
                    "bedingung von '{}' muss vom typ bool sein, gefunden {}",
                    kw,
                    self.tcx.name_of(&t)
                ),
                "es gibt keine implizite umwandlung, schreibe z. B. 'x != 0'",
            );
        }
    }

    // -------------------------------------------------------------- lvalues

    /// Prueft einen zuweisbaren Ausdruck. `None` heisst: kein lvalue (gemeldet).
    fn lvalue(&mut self, e: &Expr) -> Option<(Type, Mutability)> {
        match &e.kind {
            ExprKind::Ident(name) => {
                if let Some(v) = self.lookup_var(name) {
                    let ty = v.ty.clone();
                    let m = if v.mutable {
                        Mutability::Mutable
                    } else {
                        Mutability::Fixed(format!(
                            "'{}' ist mit 'let' gebunden und kann nicht veraendert werden",
                            name
                        ))
                    };
                    self.record(e.id, ty.clone());
                    Some((ty, m))
                } else if let Some((ty, _)) = self.consts.get(name) {
                    let ty = ty.clone();
                    self.record(e.id, ty.clone());
                    Some((
                        ty,
                        Mutability::Fixed(format!(
                            "'{}' ist eine konstante und kann nicht veraendert werden",
                            name
                        )),
                    ))
                } else {
                    self.dg
                        .error(e.span, format!("unbekannter name '{}'", name));
                    self.record(e.id, Type::Error);
                    Some((Type::Error, Mutability::Mutable))
                }
            }
            ExprKind::Field(base, name, nspan) => {
                // HOOK gc: Schreiben DURCH einen `Gc[T]` hindurch (gc.rs).
                // `let a: Gc[Knoten]` bindet den GRIFF unveraenderlich — das
                // Objekt am anderen Ende bleibt schreibbar, genau wie bei
                // `let p: *mut T` und `(*p).feld = …`.
                if let Some(bt) = self.probe(base).filter(crate::gc::ist_gc_ptr) {
                    let _ = self.expr(base, None);
                    let ty = self.field_type(&bt, name, *nspan, base.span);
                    self.record(e.id, ty.clone());
                    return Some((ty, Mutability::Mutable));
                }
                let (bt, m) = self.lvalue(base)?;
                let ty = self.field_type(&bt, name, *nspan, base.span);
                self.record(e.id, ty.clone());
                Some((ty, m))
            }
            ExprKind::Index(base, idx) => {
                let (bt, m) = self.lvalue(base)?;
                let ty = self.index_type(&bt, idx, base.span);
                self.record(e.id, ty.clone());
                Some((ty, m))
            }
            ExprKind::Unary(UnOp::Deref, inner) => {
                let t = self.expr(inner, None);
                let ty = match &t {
                    Type::Ptr { inner: i, .. } => (**i).clone(),
                    Type::Error => Type::Error,
                    other => {
                        self.dg.error(
                            e.span,
                            format!(
                                "dereferenzierung erwartet einen zeiger, gefunden {}",
                                self.tcx.name_of(other)
                            ),
                        );
                        Type::Error
                    }
                };
                self.record(e.id, ty.clone());
                Some((ty, Mutability::Mutable))
            }
            _ => {
                self.expr(e, Some(&Type::I64));
                self.dg.error(
                    e.span,
                    "linke seite ist kein zuweisbarer ausdruck (variable, feld, index oder '*zeiger')",
                );
                None
            }
        }
    }

    fn field_type(&mut self, base: &Type, name: &str, nspan: Span, bspan: Span) -> Type {
        // HOOK gc: Feldzugriff durch `Gc[T]` hindurch (gc.rs, SPEC 3.5.1). Ein
        // Gc-Zeiger wird gefolgt, ohne `(*p).feld` — er ist erstklassig.
        if let Some(i) = crate::gc::hook_field_base(base) {
            return self.field_type(&Type::Struct(i), name, nspan, bspan);
        }
        match base {
            Type::Struct(i) => match self.tcx.structs.get(*i).and_then(|s| s.field(name)) {
                Some(f) => f.ty.clone(),
                None => {
                    let sname = self
                        .tcx
                        .structs
                        .get(*i)
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| "<struct>".to_string());
                    self.dg
                        .error(nspan, format!("struct '{}' hat kein feld '{}'", sname, name));
                    Type::Error
                }
            },
            Type::Error => Type::Error,
            Type::Ptr { inner, .. } if matches!(**inner, Type::Struct(_)) => {
                self.dg.error_note(
                    bspan,
                    format!(
                        "feldzugriff auf zeigertyp {}",
                        self.tcx.name_of(base)
                    ),
                    "es gibt keine automatische dereferenzierung, schreibe '(*p).feld'",
                );
                Type::Error
            }
            other => {
                self.dg.error(
                    bspan,
                    format!(
                        "feldzugriff auf nicht-struct-typ {}",
                        self.tcx.name_of(other)
                    ),
                );
                Type::Error
            }
        }
    }

    fn index_type(&mut self, base: &Type, idx: &Expr, bspan: Span) -> Type {
        let it = self.expr(idx, Some(&Type::Usize));
        if !it.is_error() && it != Type::Usize {
            self.dg.error_note(
                idx.span,
                format!(
                    "index muss vom typ usize sein, gefunden {}",
                    self.tcx.name_of(&it)
                ),
                "schreibe z. B. 'a[i as usize]'",
            );
        }
        match base {
            Type::Array(e, _) => (**e).clone(),
            Type::Error => Type::Error,
            other => {
                self.dg.error(
                    bspan,
                    format!("index auf nicht-array-typ {}", self.tcx.name_of(other)),
                );
                Type::Error
            }
        }
    }

    // ------------------------------------------------------------- Ausdruecke

    pub(crate) fn record(&mut self, id: ExprId, ty: Type) {
        if let Some(slot) = self.expr_types.get_mut(id as usize) {
            *slot = ty;
        }
    }

    /// Gibt allen Teilausdruecken einen konkreten Typ, ohne inhaltlich zu
    /// pruefen — benutzt nach einem bereits gemeldeten Fehler, damit keine
    /// Folgefehlerlawine ("typ des literals ...") entsteht.
    pub(crate) fn type_out_expr(&mut self, e: &Expr) {
        self.expr(e, Some(&Type::I64));
    }

    pub(crate) fn expr(&mut self, e: &Expr, hint: Option<&Type>) -> Type {
        if self.depth >= MAX_DEPTH {
            self.dg.error(
                e.span,
                "ausdruck ist zu tief verschachtelt (mehr als 200 ebenen)",
            );
            self.record(e.id, Type::Error);
            return Type::Error;
        }
        self.depth += 1;
        let t = self.expr_inner(e, hint);
        self.depth -= 1;
        self.record(e.id, t.clone());
        t
    }

    fn expr_inner(&mut self, e: &Expr, hint: Option<&Type>) -> Type {
        match &e.kind {
            ExprKind::Int(v) => match hint {
                Some(t) if t.is_concrete_int() => {
                    if !lit_fits(*v, t) {
                        self.dg.error(
                            e.span,
                            format!(
                                "ganzzahlliteral {} passt nicht in den typ {}",
                                v,
                                self.tcx.name_of(t)
                            ),
                        );
                    }
                    t.clone()
                }
                Some(Type::Error) => Type::Error,
                Some(Type::Bool) => {
                    self.dg.error_note(
                        e.span,
                        "hier wird ein wahrheitswert vom typ bool erwartet, gefunden ein ganzzahlliteral",
                        "es gibt keine implizite umwandlung, schreibe z. B. 'x != 0'",
                    );
                    Type::Error
                }
                _ => {
                    self.dg.error_note(
                        e.span,
                        "typ des ganzzahlliterals ist nicht ableitbar",
                        "gib den typ an, z. B. '5 as i32' oder 'let x: i32 = 5'",
                    );
                    Type::Error
                }
            },
            ExprKind::Bool(_) => Type::Bool,
            ExprKind::Ident(name) => {
                if let Some(v) = self.lookup_var(name) {
                    v.ty.clone()
                } else if let Some((t, _)) = self.consts.get(name) {
                    t.clone()
                } else if self.fns.contains_key(name) {
                    self.dg.error_note(
                        e.span,
                        format!("'{}' ist eine funktion und kein wert", name),
                        "funktionszeiger gibt es in stufe 0 nicht, rufe sie mit '(...)' auf",
                    );
                    Type::Error
                } else {
                    self.dg
                        .error(e.span, format!("unbekannter name '{}'", name));
                    Type::Error
                }
            }
            ExprKind::Unary(op, inner) => self.unary(e, *op, inner, hint),
            ExprKind::Binary(op, l, r) => self.binary(e, *op, l, r, hint),
            ExprKind::Field(base, name, nspan) => {
                let bt = self.expr(base, None);
                self.field_type(&bt, name, *nspan, base.span)
            }
            ExprKind::Index(base, idx) => {
                let bt = self.expr(base, None);
                self.index_type(&bt, idx, base.span)
            }
            ExprKind::Call(name, args, nspan) => {
                // HOOK fehlerunionen: `try`, `catch`, `Fehlermenge::Variante` (errors.rs)
                if let Some(t) = crate::errors::hook_call(self, e.id, name, args, *nspan, e.span) {
                    return t;
                }
                self.call(name, args, *nspan, e.span)
            }
            ExprKind::Syscall(args) => {
                if args.is_empty() || args.len() > 7 {
                    self.dg.error_note(
                        e.span,
                        format!(
                            "'syscall' erwartet 1 bis 7 argumente (nummer und bis zu 6 werte), gefunden {}",
                            args.len()
                        ),
                        "aufruf: syscall(nr, a1, ..., a6)",
                    );
                }
                for a in args {
                    let t = self.expr(a, Some(&Type::I64));
                    if !t.is_error() && !t.is_concrete_int() && !t.is_ptr() {
                        self.dg.error(
                            a.span,
                            format!(
                                "'syscall'-argument muss ganzzahl- oder zeigertyp sein, gefunden {}",
                                self.tcx.name_of(&t)
                            ),
                        );
                    }
                }
                Type::I64
            }
            ExprKind::Cast(inner, te) => {
                let dst = self.resolve_ty(te);
                let inner_hint = if dst.is_concrete_int() {
                    Some(dst.clone())
                } else if dst == Type::Bool {
                    Some(Type::I64)
                } else if dst.is_ptr() {
                    Some(Type::Usize)
                } else {
                    None
                };
                let src = self.expr(inner, inner_hint.as_ref());
                if src.is_error() || dst.is_error() {
                    return dst;
                }
                let ok = cast_kind(&src) && cast_kind(&dst);
                if !ok {
                    self.dg.error(
                        e.span,
                        format!(
                            "umwandlung von {} nach {} ist nicht erlaubt",
                            self.tcx.name_of(&src),
                            self.tcx.name_of(&dst)
                        ),
                    );
                    return Type::Error;
                }
                dst
            }
            ExprKind::StructLit(name, fields, nspan) => self.struct_lit(name, fields, *nspan),
            ExprKind::ArrayRepeat(val, count) => {
                let ct = self.expr(count, Some(&Type::Usize));
                let n = if ct.is_error() || !ct.is_concrete_int() {
                    if !ct.is_error() {
                        self.dg.error(
                            count.span,
                            format!(
                                "die laenge eines wiederholungsliterals muss eine ganzzahl sein, gefunden {}",
                                self.tcx.name_of(&ct)
                            ),
                        );
                    }
                    0
                } else {
                    match self.eval_const(count) {
                        Ok(v) if v > 0 => v as u64,
                        Ok(_) => {
                            self.dg.error(
                                count.span,
                                "die laenge eines wiederholungsliterals muss groesser als null sein",
                            );
                            0
                        }
                        Err((sp, msg)) => {
                            self.dg.error(sp, msg);
                            0
                        }
                    }
                };
                let elem_hint = match hint {
                    Some(Type::Array(et, _)) => Some((**et).clone()),
                    _ => None,
                };
                let vt = self.expr(val, elem_hint.as_ref());
                if n == 0 || vt.is_error() {
                    return Type::Error;
                }
                if let Some(Type::Array(et, want_n)) = hint {
                    if !assignable(&vt, et) {
                        self.dg.error(
                            val.span,
                            format!(
                                "element hat typ {}, erwartet {}",
                                self.tcx.name_of(&vt),
                                self.tcx.name_of(et)
                            ),
                        );
                    }
                    if *want_n != n {
                        self.dg.error(
                            count.span,
                            format!("erwartet werden {} elemente, das literal hat {}", want_n, n),
                        );
                    }
                }
                Type::Array(Box::new(vt), n)
            }
            ExprKind::ArrayLit(elems) => match hint {
                Some(Type::Array(et, n)) => {
                    if elems.len() as u64 != *n {
                        self.dg.error(
                            e.span,
                            format!(
                                "array-literal hat {} elemente, erwartet werden {}",
                                elems.len(),
                                n
                            ),
                        );
                    }
                    for el in elems {
                        let t = self.expr(el, Some(et));
                        if !assignable(&t, et) {
                            self.dg.error(
                                el.span,
                                format!(
                                    "element hat typ {}, erwartet {}",
                                    self.tcx.name_of(&t),
                                    self.tcx.name_of(et)
                                ),
                            );
                        }
                    }
                    Type::Array(et.clone(), *n)
                }
                _ => {
                    for el in elems {
                        self.type_out_expr(el);
                    }
                    self.dg.error_note(
                        e.span,
                        "typ des array-literals ist nicht ableitbar",
                        "gib den typ an, z. B. 'var a: [i32; 3] = [1, 2, 3]'",
                    );
                    Type::Error
                }
            },
        }
    }

    fn unary(&mut self, e: &Expr, op: UnOp, inner: &Expr, hint: Option<&Type>) -> Type {
        match op {
            UnOp::Neg => {
                let h = self
                    .probe(inner)
                    .or_else(|| hint.filter(|t| t.is_concrete_int()).cloned());
                let t = self.expr(inner, h.as_ref());
                if t.is_error() {
                    return Type::Error;
                }
                if !t.is_concrete_int() {
                    self.dg.error(
                        e.span,
                        format!(
                            "unaeres '-' erwartet einen ganzzahltyp, gefunden {}",
                            self.tcx.name_of(&t)
                        ),
                    );
                    return Type::Error;
                }
                t
            }
            UnOp::Not => {
                let t = self.expr(inner, Some(&Type::Bool));
                if t.is_error() {
                    return Type::Error;
                }
                if t != Type::Bool {
                    self.dg.error_note(
                        e.span,
                        format!(
                            "unaeres '!' erwartet den typ bool, gefunden {}",
                            self.tcx.name_of(&t)
                        ),
                        "bitweise negation gibt es in stufe 0 nicht, schreibe 'x ^ -1'",
                    );
                    return Type::Error;
                }
                Type::Bool
            }
            UnOp::AddrOf => {
                let (t, _m) = match self.lvalue(inner) {
                    Some(x) => x,
                    None => return Type::Error,
                };
                if t.is_error() {
                    return Type::Error;
                }
                Type::ptr(t, true)
            }
            UnOp::Deref => {
                let t = self.expr(inner, None);
                match &t {
                    Type::Ptr { inner: i, .. } => (**i).clone(),
                    Type::Error => Type::Error,
                    other => {
                        self.dg.error(
                            e.span,
                            format!(
                                "dereferenzierung erwartet einen zeiger, gefunden {}",
                                self.tcx.name_of(other)
                            ),
                        );
                        Type::Error
                    }
                }
            }
        }
    }

    fn binary(&mut self, e: &Expr, op: BinOp, l: &Expr, r: &Expr, hint: Option<&Type>) -> Type {
        // HOOK fehlerunionen: Vergleich zweier Fehlerwerte (errors.rs)
        if let Some(t) = crate::errors::hook_binary(self, op, l, r, e.span) {
            return t;
        }
        if op.is_logic() {
            let lt = self.expr(l, Some(&Type::Bool));
            let rt = self.expr(r, Some(&Type::Bool));
            for (t, sp) in [(lt, l.span), (rt, r.span)] {
                if !t.is_error() && t != Type::Bool {
                    self.dg.error(
                        sp,
                        format!(
                            "operator '{}' erwartet operanden vom typ bool, gefunden {}",
                            op.text(),
                            self.tcx.name_of(&t)
                        ),
                    );
                }
            }
            return Type::Bool;
        }
        if op.is_cmp() {
            let want = self.probe(l).or_else(|| self.probe(r));
            let lt = self.expr(l, want.as_ref());
            let rt = self.expr(r, want.as_ref());
            if lt.is_error() || rt.is_error() {
                return Type::Bool;
            }
            // HOOK gc: Identitaetsvergleich zweier verwandter Gc-Zeiger (gc.rs)
            let same = compatible(&lt, &rt) || crate::gc::ist_verwandt(&lt, &rt);
            if !same {
                self.dg.error(
                    e.span,
                    format!(
                        "vergleich zwischen unterschiedlichen typen {} und {}",
                        self.tcx.name_of(&lt),
                        self.tcx.name_of(&rt)
                    ),
                );
                return Type::Bool;
            }
            let eq_only = matches!(op, BinOp::Eq | BinOp::Ne);
            let ok = lt.is_concrete_int() || (eq_only && (lt == Type::Bool || lt.is_ptr()));
            if !ok {
                self.dg.error(
                    e.span,
                    format!(
                        "operator '{}' ist fuer den typ {} nicht definiert",
                        op.text(),
                        self.tcx.name_of(&lt)
                    ),
                );
            }
            return Type::Bool;
        }
        if matches!(op, BinOp::Shl | BinOp::Shr) {
            let h = self
                .probe(l)
                .or_else(|| hint.filter(|t| t.is_concrete_int()).cloned());
            let lt = self.expr(l, h.as_ref());
            let rh = self.probe(r).or_else(|| {
                if lt.is_concrete_int() {
                    Some(lt.clone())
                } else {
                    Some(Type::I64)
                }
            });
            let rt = self.expr(r, rh.as_ref());
            if lt.is_error() || rt.is_error() {
                return if lt.is_error() { Type::Error } else { lt };
            }
            if !lt.is_concrete_int() || !rt.is_concrete_int() {
                self.dg.error(
                    e.span,
                    format!(
                        "operator '{}' erwartet ganzzahltypen, gefunden {} und {}",
                        op.text(),
                        self.tcx.name_of(&lt),
                        self.tcx.name_of(&rt)
                    ),
                );
                return Type::Error;
            }
            return lt;
        }
        // Arithmetik und Bitoperationen: gleicher Ganzzahltyp auf beiden Seiten
        // Der Typ eines bereits typisierten Operanden hat Vorrang vor dem
        // Kontexthinweis — so meldet `let x: i64 = a + 1` (a: i32) den echten
        // Fehler an der Zuweisung statt einen verwirrenden Operandenfehler.
        let want = self
            .probe(l)
            .or_else(|| self.probe(r))
            .or_else(|| hint.filter(|t| t.is_concrete_int()).cloned());
        let lt = self.expr(l, want.as_ref());
        let rt = self.expr(r, want.as_ref());
        if lt.is_error() || rt.is_error() {
            return Type::Error;
        }
        if !lt.is_concrete_int() || !rt.is_concrete_int() || lt != rt {
            self.dg.error_note(
                e.span,
                format!(
                    "operator '{}' erwartet zwei operanden desselben ganzzahltyps, gefunden {} und {}",
                    op.text(),
                    self.tcx.name_of(&lt),
                    self.tcx.name_of(&rt)
                ),
                "es gibt keine implizite umwandlung, benutze 'as'",
            );
            return Type::Error;
        }
        lt
    }

    fn call(&mut self, name: &str, args: &[Expr], nspan: Span, espan: Span) -> Type {
        // HOOK types: `Enum::Variante(..)` und `match` (sema_match.rs)
        if let Some(t) = crate::sema_match::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        // HOOK constant-time: select/barrier/secure_zero (ct.rs, SPEC §9.2/§9.3)
        if let Some(t) = crate::ct::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        // HOOK gc: `gc C{…}`, `weak(g)`, `stark(w)`, `x.as?[C]` und die
        // Sammler-Intrinsics (gc.rs, SPEC 3.5)
        if let Some(t) = crate::gc::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        let sig = match self.fns.get(name) {
            Some(s) => s.clone(),
            None => {
                for a in args {
                    self.type_out_expr(a);
                }
                if self.lookup_var(name).is_some() || self.consts.contains_key(name) {
                    self.dg.error(
                        nspan,
                        format!("'{}' ist keine funktion und kann nicht aufgerufen werden", name),
                    );
                } else {
                    self.dg
                        .error(nspan, format!("unbekannte funktion '{}'", name));
                }
                return Type::Error;
            }
        };
        if args.len() != sig.params.len() {
            self.dg.error(
                espan,
                format!(
                    "funktion '{}' erwartet {} argument(e), gefunden {}",
                    name,
                    sig.params.len(),
                    args.len()
                ),
            );
        }
        for (i, a) in args.iter().enumerate() {
            match sig.params.get(i) {
                Some(p) => {
                    // HOOK fehlerunionen: implizite Umwandlung (errors.rs)
                    if crate::errors::hook_coerce(self, a, p) {
                        continue;
                    }
                    let t = self.expr(a, Some(p));
                    if !assignable(&t, p) {
                        self.dg.error(
                            a.span,
                            format!(
                                "argument {} von '{}' hat typ {}, erwartet {}",
                                i + 1,
                                name,
                                self.tcx.name_of(&t),
                                self.tcx.name_of(p)
                            ),
                        );
                    }
                }
                None => {
                    self.type_out_expr(a);
                }
            }
        }
        sig.ret
    }

    fn struct_lit(&mut self, name: &str, fields: &[(String, Expr, Span)], nspan: Span) -> Type {
        let idx = match self.tcx.lookup(name) {
            Some(i) => i,
            None => {
                for (_, e, _) in fields {
                    self.type_out_expr(e);
                }
                self.dg
                    .error(nspan, format!("unbekannter struct-typ '{}'", name));
                return Type::Error;
            }
        };
        let def_fields: Vec<(String, Type)> = self
            .tcx
            .structs
            .get(idx)
            .map(|s| s.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect())
            .unwrap_or_default();
        let mut seen: HashSet<String> = HashSet::new();
        for (fname, fexpr, fspan) in fields {
            match def_fields.iter().find(|(n, _)| n == fname) {
                Some((_, ft)) => {
                    if !seen.insert(fname.clone()) {
                        self.dg.error(
                            *fspan,
                            format!("feld '{}' ist mehrfach angegeben", fname),
                        );
                    }
                    // HOOK fehlerunionen: implizite Umwandlung (errors.rs)
                    if crate::errors::hook_coerce(self, fexpr, ft) {
                        continue;
                    }
                    let t = self.expr(fexpr, Some(ft));
                    if !assignable(&t, ft) {
                        self.dg.error(
                            fexpr.span,
                            format!(
                                "feld '{}' hat typ {}, erwartet {}",
                                fname,
                                self.tcx.name_of(&t),
                                self.tcx.name_of(ft)
                            ),
                        );
                    }
                }
                None => {
                    self.type_out_expr(fexpr);
                    self.dg.error(
                        *fspan,
                        format!("struct '{}' hat kein feld '{}'", name, fname),
                    );
                }
            }
        }
        let missing: Vec<String> = def_fields
            .iter()
            .filter(|(n, _)| !seen.contains(n))
            .map(|(n, _)| n.clone())
            .collect();
        if !missing.is_empty() {
            self.dg.error(
                nspan,
                format!(
                    "struct-literal '{}' fehlt das feld '{}'",
                    name,
                    missing.join("', '")
                ),
            );
        }
        Type::Struct(idx)
    }

    /// Ermittelt den Typ eines Ausdrucks ohne Fehler zu melden und ohne die
    /// Typtabelle zu beschreiben. Wird gebraucht, um bei `a + 1` den Typ des
    /// Literals aus dem anderen Operanden zu gewinnen.
    fn probe(&self, e: &Expr) -> Option<Type> {
        self.probe_d(e, 0)
    }

    fn probe_d(&self, e: &Expr, d: u32) -> Option<Type> {
        if d >= MAX_DEPTH {
            return None;
        }
        match &e.kind {
            ExprKind::Int(_) => None,
            ExprKind::Bool(_) => Some(Type::Bool),
            ExprKind::Ident(n) => {
                if let Some(v) = self.lookup_var(n) {
                    Some(v.ty.clone())
                } else {
                    self.consts.get(n).map(|(t, _)| t.clone())
                }
            }
            ExprKind::Unary(op, inner) => match op {
                UnOp::Neg => self.probe_d(inner, d + 1),
                UnOp::Not => Some(Type::Bool),
                UnOp::AddrOf => self.probe_d(inner, d + 1).map(|t| Type::ptr(t, true)),
                UnOp::Deref => match self.probe_d(inner, d + 1) {
                    Some(Type::Ptr { inner: i, .. }) => Some((*i).clone()),
                    _ => None,
                },
            },
            ExprKind::Binary(op, l, r) => {
                if op.is_cmp() || op.is_logic() {
                    Some(Type::Bool)
                } else if matches!(op, BinOp::Shl | BinOp::Shr) {
                    self.probe_d(l, d + 1)
                } else {
                    self.probe_d(l, d + 1).or_else(|| self.probe_d(r, d + 1))
                }
            }
            ExprKind::Field(base, name, _) => {
                let bt = self.probe_d(base, d + 1)?;
                // HOOK gc: Feldzugriff durch `Gc[T]` hindurch (gc.rs)
                let idx = match bt {
                    Type::Struct(i) => Some(i),
                    ref t => crate::gc::hook_field_base(t),
                };
                idx.and_then(|i| self.tcx.structs.get(i))
                    .and_then(|s| s.field(name))
                    .map(|f| f.ty.clone())
            }
            ExprKind::Index(base, _) => match self.probe_d(base, d + 1) {
                Some(Type::Array(el, _)) => Some((*el).clone()),
                _ => None,
            },
            ExprKind::Call(name, args, _) => {
                // HOOK fehlerunionen: `try a`/`a catch b` liefern den
                // Erfolgstyp der Fehlerunion (errors.rs)
                if crate::errors::is_result_call(name) {
                    let inner = args.first().and_then(|a| self.probe_d(a, d + 1))?;
                    return crate::errors::success_type(&inner);
                }
                // HOOK gc: Typ von `weak(g)`, `stark(w)` und `x.as?[C]` OHNE
                // Pruefung, damit ein Literal daneben seinen Typ bekommt (gc.rs)
                let arg0 = args.first().and_then(|a| self.probe_d(a, d + 1));
                if let Some(t) = crate::gc::probe_typ(name, arg0.as_ref()) {
                    return Some(t);
                }
                self.fns.get(name).map(|s| s.ret.clone())
            }
            ExprKind::Syscall(_) => Some(Type::I64),
            ExprKind::Cast(_, te) => self.resolve_ty_quiet(te),
            ExprKind::StructLit(name, _, _) => self.tcx.lookup(name).map(Type::Struct),
            ExprKind::ArrayRepeat(..) => None,
            ExprKind::ArrayLit(els) => {
                let first = els.first()?;
                let et = self.probe_d(first, d + 1)?;
                Some(Type::Array(Box::new(et), els.len() as u64))
            }
        }
    }

    // ------------------------------------------------------------------ Typen

    pub(crate) fn resolve_ty(&mut self, te: &TypeExpr) -> Type {
        self.resolve_ty_d(te, 0)
    }

    fn resolve_ty_d(&mut self, te: &TypeExpr, d: u32) -> Type {
        if d >= MAX_DEPTH {
            self.dg
                .error(te.span(), "typ ist zu tief verschachtelt (mehr als 200 ebenen)");
            return Type::Error;
        }
        // HOOK fehlerunionen: Fehlerunion `E!T` (errors.rs)
        if let Some(t) = crate::errors::hook_resolve_ty(self, te) {
            return t;
        }
        // HOOK gc: `Gc[C]`, `GcWeak[C]` und der verbotene Gebrauch eines
        // `gc class`-Namens als gewoehnlicher Wert (gc.rs)
        if let Some(t) = crate::gc::hook_resolve_ty(self, te) {
            return t;
        }
        match te {
            TypeExpr::Named(name, span) => match prim_type(name) {
                Some(t) => t,
                None => match self.tcx.lookup(name) {
                    Some(i) => Type::Struct(i),
                    None => {
                        self.dg.error(*span, format!("unbekannter typ '{}'", name));
                        Type::Error
                    }
                },
            },
            TypeExpr::Ptr { mutable, inner, .. } => {
                let t = self.resolve_ty_d(inner, d + 1);
                if t.is_error() {
                    return Type::Error;
                }
                Type::ptr(t, *mutable)
            }
            TypeExpr::Array { elem, len, span } => {
                let t = self.resolve_ty_d(elem, d + 1);
                if t.is_error() {
                    return Type::Error;
                }
                if *len == 0 {
                    self.dg.error(*span, "arraylaenge muss groesser als null sein");
                    return Type::Error;
                }
                Type::Array(Box::new(t), *len)
            }
        }
    }

    /// Typaufloesung ohne Fehlermeldung (fuer `probe`).
    fn resolve_ty_quiet(&self, te: &TypeExpr) -> Option<Type> {
        match te {
            TypeExpr::Named(name, _) => match prim_type(name) {
                Some(t) => Some(t),
                None => self.tcx.lookup(name).map(Type::Struct),
            },
            TypeExpr::Ptr { mutable, inner, .. } => {
                self.resolve_ty_quiet(inner).map(|t| Type::ptr(t, *mutable))
            }
            TypeExpr::Array { elem, len, .. } => self
                .resolve_ty_quiet(elem)
                .map(|t| Type::Array(Box::new(t), *len)),
        }
    }

    // ------------------------------------------------------- Konstantenwerte

    fn eval_const(&self, e: &Expr) -> Result<i128, (Span, String)> {
        self.eval_const_d(e, 0)
    }

    fn eval_const_d(&self, e: &Expr, d: u32) -> Result<i128, (Span, String)> {
        let nope = |msg: &str| Err((e.span, msg.to_string()));
        if d >= MAX_DEPTH {
            return nope("konstanter ausdruck ist zu tief verschachtelt");
        }
        match &e.kind {
            ExprKind::Int(v) => Ok(*v),
            ExprKind::Bool(b) => Ok(if *b { 1 } else { 0 }),
            ExprKind::Ident(n) => match self.consts.get(n) {
                Some((_, v)) => Ok(*v),
                None => Err((
                    e.span,
                    format!("'{}' ist keine bereits deklarierte konstante", n),
                )),
            },
            ExprKind::Unary(op, inner) => {
                let v = self.eval_const_d(inner, d + 1)?;
                match op {
                    UnOp::Neg => Ok(-v),
                    UnOp::Not => Ok(if v == 0 { 1 } else { 0 }),
                    _ => nope("konstanter ausdruck darf keine zeiger benutzen"),
                }
            }
            ExprKind::Binary(op, l, r) => {
                let a = self.eval_const_d(l, d + 1)?;
                if matches!(op, BinOp::LAnd) && a == 0 {
                    return Ok(0);
                }
                if matches!(op, BinOp::LOr) && a != 0 {
                    return Ok(1);
                }
                let b = self.eval_const_d(r, d + 1)?;
                let lty = self
                    .expr_types
                    .get(l.id as usize)
                    .cloned()
                    .unwrap_or(Type::I64);
                let signed = lty.is_signed();
                let v = match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div | BinOp::Rem => {
                        if b == 0 {
                            return Err((
                                e.span,
                                "division durch null im konstanten ausdruck".to_string(),
                            ));
                        }
                        if *op == BinOp::Div {
                            a / b
                        } else {
                            a % b
                        }
                    }
                    BinOp::And => a & b,
                    BinOp::Or => a | b,
                    BinOp::Xor => a ^ b,
                    BinOp::Shl => {
                        if !(0..128).contains(&b) {
                            return Err((
                                e.span,
                                "verschiebeweite im konstanten ausdruck ist zu gross".to_string(),
                            ));
                        }
                        a << b
                    }
                    BinOp::Shr => {
                        if !(0..128).contains(&b) {
                            return Err((
                                e.span,
                                "verschiebeweite im konstanten ausdruck ist zu gross".to_string(),
                            ));
                        }
                        if signed {
                            a >> b
                        } else {
                            ((a as u128) >> b) as i128
                        }
                    }
                    BinOp::Eq => bit(a == b),
                    BinOp::Ne => bit(a != b),
                    BinOp::Lt => bit(a < b),
                    BinOp::Le => bit(a <= b),
                    BinOp::Gt => bit(a > b),
                    BinOp::Ge => bit(a >= b),
                    BinOp::LAnd => bit(b != 0),
                    BinOp::LOr => bit(b != 0),
                };
                let rty = self
                    .expr_types
                    .get(e.id as usize)
                    .cloned()
                    .unwrap_or(Type::I64);
                Ok(wrap(v, &rty))
            }
            ExprKind::Cast(inner, _) => {
                let v = self.eval_const_d(inner, d + 1)?;
                let dst = self
                    .expr_types
                    .get(e.id as usize)
                    .cloned()
                    .unwrap_or(Type::I64);
                if dst == Type::Bool {
                    return Ok(bit(v != 0));
                }
                if dst.is_ptr() {
                    return nope("zeiger sind im konstanten ausdruck nicht erlaubt");
                }
                Ok(wrap(v, &dst))
            }
            _ => nope("konstanter ausdruck muss zur uebersetzungszeit auswertbar sein (nur literale, konstanten und operatoren)"),
        }
    }
}

fn bit(b: bool) -> i128 {
    if b {
        1
    } else {
        0
    }
}

fn prim_type(name: &str) -> Option<Type> {
    Some(match name {
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "usize" => Type::Usize,
        "isize" => Type::Isize,
        "bool" => Type::Bool,
        _ => return None,
    })
}

/// Wert auf die Breite/Signiertheit des Zieltyps zurechtschneiden.
fn wrap(v: i128, t: &Type) -> i128 {
    let bits = t.bits();
    if bits == 0 {
        return v;
    }
    if bits >= 64 {
        return if t.is_signed() {
            v as i64 as i128
        } else {
            (v as u64) as i128
        };
    }
    let m = (1i128 << bits) - 1;
    let x = v & m;
    if t.is_signed() && (x >> (bits - 1)) & 1 == 1 {
        x - (1i128 << bits)
    } else {
        x
    }
}

/// Passt das Literal (vorzeichenbehaftet ODER vorzeichenlos gelesen) in den Typ?
fn lit_fits(v: i128, t: &Type) -> bool {
    let bits = t.bits() as i128;
    if bits >= 64 {
        return v >= i64::MIN as i128 && v <= u64::MAX as i128;
    }
    let ub = 1i128 << bits;
    v >= -(ub / 2) && v < ub
}

/// Darf dieser Typ an einer `as`-Umwandlung teilnehmen?
fn cast_kind(t: &Type) -> bool {
    t.is_concrete_int() || *t == Type::Bool || t.is_ptr()
}

/// Zuweisungsvertraeglichkeit — es gibt KEINE impliziten Umwandlungen. Einzige
/// Nachsicht: die `mut`-Kennzeichnung eines Zeigers wird nicht geprueft (Stufe 0
/// hat keinen Mutabilitaetspruefer fuer Zeigerziele).
fn assignable(got: &Type, want: &Type) -> bool {
    if got.is_error() || want.is_error() {
        return true;
    }
    // HOOK gc: kostenlose Aufwaertsumwandlung `Gc[Abgeleitet]` -> `Gc[Basis]`
    // (gc.rs, SPEC 4.4). NUR diese Richtung; abwaerts geht ausschliesslich
    // ueber das gepruefte `x.as?[C]`.
    if crate::gc::ist_aufwaerts(got, want) {
        return true;
    }
    compatible(got, want)
}

fn compatible(a: &Type, b: &Type) -> bool {
    if a.is_error() || b.is_error() {
        return true;
    }
    match (a, b) {
        (Type::Ptr { inner: x, .. }, Type::Ptr { inner: y, .. }) => compatible(x, y),
        _ => a == b,
    }
}

/// Sammelt alle Structs, die `ty` DEM WERT NACH enthaelt (Zeiger nicht).
fn collect_value_deps(ty: &Type, out: &mut Vec<usize>) {
    match ty {
        Type::Struct(i) => out.push(*i),
        Type::Array(e, _) => collect_value_deps(e, out),
        _ => {}
    }
}

/// Tiefensuche: erkennt Zyklen und liefert eine topologische Reihenfolge
/// (Abhaengigkeiten zuerst) fuer die Layoutberechnung.
fn find_cycles(
    i: usize,
    deps: &[Vec<usize>],
    state: &mut Vec<u8>,
    order: &mut Vec<usize>,
    bad: &mut HashSet<usize>,
) {
    match state.get(i) {
        Some(2) => return,
        Some(1) => {
            bad.insert(i);
            return;
        }
        None => return,
        _ => {}
    }
    state[i] = 1;
    if let Some(list) = deps.get(i) {
        for d in list.clone() {
            find_cycles(d, deps, state, order, bad);
            if bad.contains(&d) {
                bad.insert(i);
            }
        }
    }
    state[i] = 2;
    order.push(i);
}

/// Erreichbarkeitsanalyse: endet JEDER pfad des blocks mit 'return'?
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
        // Eine 'while true'-Schleife OHNE 'break' verlaesst den Rumpf nie.
        Stmt::While { cond, body, .. } => {
            matches!(cond.kind, ExprKind::Bool(true)) && !block_breaks(body)
        }
        // HOOK types: ein vollstaendiges 'match', dessen Faelle alle
        // zurueckkehren, kehrt selbst zurueck (sema_match.rs).
        Stmt::Expr(e) => crate::sema_match::match_returns(e),
        _ => false,
    }
}

/// Enthaelt der Block ein `break`, das DIESE Schleife verlaesst (also keines
/// aus einer inneren Schleife)?
fn block_breaks(b: &Block) -> bool {
    b.stmts.iter().any(stmt_breaks)
}

fn stmt_breaks(s: &Stmt) -> bool {
    match s {
        Stmt::Break(_) => true,
        Stmt::Block(b) => block_breaks(b),
        Stmt::If { then, els, .. } => {
            block_breaks(then) || els.as_deref().map(stmt_breaks).unwrap_or(false)
        }
        // 'break' in einer inneren Schleife verlaesst nur diese.
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Selbstpruefung
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        Block, ConstDecl, Expr, ExprKind, FnDecl, Param, Program, Stmt, StructDecl, TypeExpr,
    };

    fn sp() -> Span {
        Span::new(1, 1, 1)
    }

    struct B {
        next: u32,
    }

    impl B {
        fn new() -> B {
            B { next: 0 }
        }
        fn e(&mut self, k: ExprKind) -> Expr {
            let id = self.next;
            self.next += 1;
            Expr { id, span: sp(), kind: k }
        }
        fn int(&mut self, v: i128) -> Expr {
            self.e(ExprKind::Int(v))
        }
        fn id(&mut self, n: &str) -> Expr {
            self.e(ExprKind::Ident(n.to_string()))
        }
    }

    // ------------------------------------------------------------------
    // Wiedereintritt in die Pruefphasen (DESIGNZIELE.md §7)
    // ------------------------------------------------------------------

    /// Baut einen Pruefer im Zustand *nach* dem ersten Durchlauf.
    fn checker_nach_erstem_lauf<'d>(
        dg: &'d mut Diags,
        erstes: &Program,
    ) -> Checker<'d> {
        let mut ck = Checker {
            dg,
            tcx: TypeCtx::new(),
            fns: HashMap::new(),
            consts: HashMap::new(),
            expr_types: vec![Type::Error; erstes.expr_count as usize],
            scopes: Vec::new(),
            ret: Type::Void,
            depth: 0,
            must_consume_fns: HashSet::new(),
        };
        ck.run(erstes);
        ck
    }

    #[test]
    fn pruefphasen_nehmen_nachtraeglich_erzeugte_elemente_an() {
        // Erster Durchlauf: nur `main` und `basis`.
        let mut b = B::new();
        let ret_basis = b.int(7);
        let basis = FnDecl {
            name: "basis".to_string(),
            params: Vec::new(),
            ret: Some(named("i32")),
            body: blk(vec![Stmt::Return { value: Some(ret_basis), span: sp() }]),
            span: sp(),
            attrs: Vec::new(),
        };
        let ret_main = b.int(0);
        let erstes = Program {
            funcs: vec![basis, main_fn(vec![Stmt::Return { value: Some(ret_main), span: sp() }])],
            expr_count: b.next,
            ..Default::default()
        };

        let mut dg = Diags::new("test.fi", "");
        let mut ck = checker_nach_erstem_lauf(&mut dg, &erstes);
        assert!(!ck.dg.has_errors(), "erster Durchlauf muss fehlerfrei sein");
        assert!(ck.fns.contains_key("basis"));
        assert!(!ck.fns.contains_key("spaeter"));

        // Zweiter Durchlauf: eine Funktion, die es beim ersten Mal noch nicht
        // gab und die auf eine Funktion des ersten Durchlaufs zugreift.
        // Genau das muss `comptime emit` spaeter tun.
        let ruf = b.e(ExprKind::Call("basis".to_string(), Vec::new(), sp()));
        let nachtrag = Program {
            funcs: vec![FnDecl {
                name: "spaeter".to_string(),
                params: Vec::new(),
                ret: Some(named("i32")),
                body: blk(vec![Stmt::Return { value: Some(ruf), span: sp() }]),
                span: sp(),
                attrs: Vec::new(),
            }],
            expr_count: b.next,
            ..Default::default()
        };
        ck.add_items(&nachtrag);

        assert!(!ck.dg.has_errors(), "Nachtrag muss fehlerfrei durchlaufen");
        assert!(ck.fns.contains_key("spaeter"), "die neue Funktion fehlt");
        // Der Aufruf hat wirklich einen Typ bekommen — die Tabelle ist mitgewachsen.
        assert_eq!(ck.expr_types.len(), b.next as usize);
        assert_eq!(ck.fns["spaeter"].ret, Type::I32);
    }

    #[test]
    fn nachtrag_wird_genauso_streng_geprueft() {
        let mut b = B::new();
        let ret_main = b.int(0);
        let erstes = Program {
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret_main), span: sp() }])],
            expr_count: b.next,
            ..Default::default()
        };
        let mut dg = Diags::new("test.fi", "");
        let mut ck = checker_nach_erstem_lauf(&mut dg, &erstes);
        assert!(!ck.dg.has_errors());

        // Nachtrag ruft etwas auf, das es nicht gibt -> derselbe Fehler wie im
        // ersten Durchlauf. Ein Nachtrag darf keine Hintertuer sein.
        let ruf = b.e(ExprKind::Call("gibt_es_nicht".to_string(), Vec::new(), sp()));
        let nachtrag = Program {
            funcs: vec![FnDecl {
                name: "kaputt".to_string(),
                params: Vec::new(),
                ret: Some(named("i32")),
                body: blk(vec![Stmt::Return { value: Some(ruf), span: sp() }]),
                span: sp(),
                attrs: Vec::new(),
            }],
            expr_count: b.next,
            ..Default::default()
        };
        ck.add_items(&nachtrag);
        assert!(ck.dg.has_errors(), "unbekannter Name im Nachtrag muss auffallen");
    }

    #[test]
    fn nachtrag_meldet_doppelte_deklaration() {
        let mut b = B::new();
        let ret_main = b.int(0);
        let erstes = Program {
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret_main), span: sp() }])],
            expr_count: b.next,
            ..Default::default()
        };
        let mut dg = Diags::new("test.fi", "");
        let mut ck = checker_nach_erstem_lauf(&mut dg, &erstes);

        let ret2 = b.int(1);
        let nachtrag = Program {
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret2), span: sp() }])],
            expr_count: b.next,
            ..Default::default()
        };
        ck.add_items(&nachtrag);
        assert!(ck.dg.has_errors(), "'main' zweimal muss ein Fehler sein");
    }

    fn named(n: &str) -> TypeExpr {
        TypeExpr::Named(n.to_string(), sp())
    }

    fn blk(stmts: Vec<Stmt>) -> Block {
        Block { stmts, span: sp() }
    }

    /// main-Funktion mit gegebenem Rumpf und `return <ret>`.
    fn main_fn(body: Vec<Stmt>) -> FnDecl {
        FnDecl {
            name: "main".to_string(),
            params: Vec::new(),
            ret: Some(named("i32")),
            body: blk(body),
            span: sp(), attrs: Vec::new(),
        }
    }

    fn run(prog: Program, src: &str) -> (Option<TypeInfo>, String) {
        let mut dg = Diags::new("test.x", src);
        let info = check(&prog, &mut dg);
        (info, dg.render())
    }

    /// Baut ein Programm, dessen main nur `return <e>` enthaelt.
    fn prog_with(b: &mut B, ret: Expr) -> Program {
        Program {
            profile: None,
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: Vec::new(),
            consts: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            expr_count: b.next,
        }
    }

    fn expect_err(prog: Program, needle: &str) {
        let (info, out) = run(prog, "");
        assert!(info.is_none(), "erwarteter fehler '{}' blieb aus", needle);
        assert!(
            out.contains(needle),
            "meldung '{}' fehlt in:\n{}",
            needle,
            out
        );
    }

    // ---- Struct-Layout (ausdruecklich gefordert) --------------------------

    fn layout_of(fields: &[(&str, &str)]) -> (Vec<u64>, u64, u64) {
        let mut b = B::new();
        let ret = b.int(0);
        let sd = StructDecl {
            name: "S".to_string(),
            fields: fields
                .iter()
                .map(|(n, t)| (n.to_string(), named(t), sp()))
                .collect(),
            span: sp(), attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: vec![sd],
            consts: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("layout-programm fehlerhaft:\n{}", out));
        let s = &info.tcx.structs[0];
        (s.fields.iter().map(|f| f.offset).collect(), s.size, s.align)
    }

    #[test]
    fn layout_u8_u32_u8() {
        let (offs, size, align) = layout_of(&[("a", "u8"), ("b", "u32"), ("c", "u8")]);
        assert_eq!(offs, vec![0, 4, 8]);
        assert_eq!(size, 12);
        assert_eq!(align, 4);
    }

    #[test]
    fn layout_packed_order() {
        let (offs, size, align) = layout_of(&[("a", "i64"), ("b", "i8"), ("c", "i16")]);
        assert_eq!(offs, vec![0, 8, 10]);
        assert_eq!(size, 16);
        assert_eq!(align, 8);
    }

    #[test]
    fn layout_bool_and_ptr() {
        let mut b = B::new();
        let ret = b.int(0);
        let sd = StructDecl {
            name: "S".to_string(),
            fields: vec![
                ("f".to_string(), named("bool"), sp()),
                (
                    "p".to_string(),
                    TypeExpr::Ptr { mutable: true, inner: Box::new(named("u8")), span: sp() },
                    sp(),
                ),
                (
                    "a".to_string(),
                    TypeExpr::Array { elem: Box::new(named("u16")), len: 3, span: sp() },
                    sp(),
                ),
            ],
            span: sp(),
            attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: vec![sd],
            consts: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("fehler:\n{}", out));
        let s = &info.tcx.structs[0];
        assert_eq!(s.fields[0].offset, 0);
        assert_eq!(s.fields[1].offset, 8);
        assert_eq!(s.fields[2].offset, 16);
        assert_eq!(s.size, 24);
        assert_eq!(s.align, 8);
    }

    #[test]
    fn layout_nested_struct() {
        let mut b = B::new();
        let ret = b.int(0);
        let inner = StructDecl {
            name: "Inner".to_string(),
            fields: vec![
                ("x".to_string(), named("u8"), sp()),
                ("y".to_string(), named("u32"), sp()),
            ],
            span: sp(), attrs: Vec::new(),
        };
        // Aeusserer Struct steht VOR dem inneren -> topologische Reihenfolge noetig.
        let outer = StructDecl {
            name: "Outer".to_string(),
            fields: vec![
                ("a".to_string(), named("u8"), sp()),
                ("i".to_string(), named("Inner"), sp()),
            ],
            span: sp(), attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: vec![outer, inner],
            consts: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("fehler:\n{}", out));
        let o = &info.tcx.structs[0];
        assert_eq!(o.fields[0].offset, 0);
        assert_eq!(o.fields[1].offset, 4);
        assert_eq!(o.size, 12);
        assert_eq!(o.align, 4);
    }

    #[test]
    fn recursive_struct_is_error() {
        let mut b = B::new();
        let ret = b.int(0);
        let a = StructDecl {
            name: "A".to_string(),
            fields: vec![("b".to_string(), named("B"), sp())],
            span: sp(), attrs: Vec::new(),
        };
        let bs = StructDecl {
            name: "B".to_string(),
            fields: vec![("a".to_string(), named("A"), sp())],
            span: sp(), attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: vec![a, bs],
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "enthaelt sich selbst");
    }

    // ---- je ein Fehlerfall pro Pruefung -----------------------------------

    #[test]
    fn ok_program_types_everything() {
        let mut b = B::new();
        let lit = b.int(7);
        let x = b.id("x");
        let one = b.int(1);
        let sum = b.e(ExprKind::Binary(BinOp::Add, Box::new(x), Box::new(one)));
        let prog = Program {
            profile: Some(("app".to_string(), sp())),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: Some(named("i32")),
                    init: lit,
                    span: sp(),
                },
                Stmt::Return { value: Some(sum), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("unerwarteter fehler:\n{}", out));
        assert_eq!(info.expr_types.len(), 4);
        for t in &info.expr_types {
            assert!(!t.is_error() && *t != Type::UntypedInt, "typ {:?}", t);
        }
        assert_eq!(info.expr_types[0], Type::I32);
    }

    #[test]
    fn untyped_literal_is_error() {
        let mut b = B::new();
        let lit = b.int(5);
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: None,
                    init: lit,
                    span: sp(),
                },
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "typ des ganzzahlliterals ist nicht ableitbar");
    }

    #[test]
    fn unknown_name_is_error() {
        let mut b = B::new();
        let x = b.id("nix");
        let prog = prog_with(&mut b, x);
        expect_err(prog, "unbekannter name 'nix'");
    }

    #[test]
    fn wrong_arg_count_is_error() {
        let mut b = B::new();
        let a1 = b.int(1);
        let call = b.e(ExprKind::Call("f".to_string(), vec![a1], sp()));
        let fret = b.int(0);
        let f = FnDecl {
            name: "f".to_string(),
            params: vec![
                Param { name: "a".to_string(), ty: named("i32"), span: sp() },
                Param { name: "b".to_string(), ty: named("i32"), span: sp() },
            ],
            ret: Some(named("i32")),
            body: blk(vec![Stmt::Return { value: Some(fret), span: sp() }]),
            span: sp(),
            attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(call), span: sp() }]), f],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "erwartet 2 argument(e), gefunden 1");
    }

    #[test]
    fn missing_return_is_error() {
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(Vec::new())],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: 0,
        };
        expect_err(prog, "erreicht das ende ohne 'return'");
    }

    #[test]
    fn assign_to_let_is_error() {
        let mut b = B::new();
        let init = b.int(1);
        let tgt = b.id("x");
        let val = b.int(2);
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: Some(named("i32")),
                    init,
                    span: sp(),
                },
                Stmt::Assign { target: tgt, value: val, span: sp() },
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "mit 'let' gebunden");
    }

    #[test]
    fn index_on_non_array_is_error() {
        let mut b = B::new();
        let init = b.int(1);
        let base = b.id("x");
        let idx = b.int(0);
        let ix = b.e(ExprKind::Index(Box::new(base), Box::new(idx)));
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: Some(named("i32")),
                    init,
                    span: sp(),
                },
                Stmt::Return { value: Some(ix), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "index auf nicht-array-typ i32");
    }

    #[test]
    fn field_on_non_struct_is_error() {
        let mut b = B::new();
        let init = b.int(1);
        let base = b.id("x");
        let f = b.e(ExprKind::Field(Box::new(base), "y".to_string(), sp()));
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: Some(named("i32")),
                    init,
                    span: sp(),
                },
                Stmt::Return { value: Some(f), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "feldzugriff auf nicht-struct-typ i32");
    }

    #[test]
    fn deref_non_pointer_is_error() {
        let mut b = B::new();
        let init = b.int(1);
        let base = b.id("x");
        let d = b.e(ExprKind::Unary(UnOp::Deref, Box::new(base)));
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: Some(named("i32")),
                    init,
                    span: sp(),
                },
                Stmt::Return { value: Some(d), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "dereferenzierung erwartet einen zeiger");
    }

    #[test]
    fn condition_must_be_bool() {
        let mut b = B::new();
        let c = b.int(1);
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::If { cond: c, then: blk(Vec::new()), els: None, span: sp() },
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "wahrheitswert vom typ bool erwartet");
    }

    #[test]
    fn mixed_int_types_are_error() {
        let mut b = B::new();
        let ia = b.int(1);
        let ib = b.int(2);
        let x = b.id("x");
        let y = b.id("y");
        let sum = b.e(ExprKind::Binary(BinOp::Add, Box::new(x), Box::new(y)));
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let { name: "x".into(), mutable: false, ty: Some(named("i32")), init: ia, span: sp() },
                Stmt::Let { name: "y".into(), mutable: false, ty: Some(named("i64")), init: ib, span: sp() },
                Stmt::Return { value: Some(sum), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "desselben ganzzahltyps, gefunden i32 und i64");
    }

    #[test]
    /// Runde 2: Aggregate an Funktionsgrenzen sind ERLAUBT (SPEC §14.1 Punkt 1
    /// gestrichen). Der Typpruefer nimmt sie an, `abi.rs` klassifiziert sie.
    fn aggregate_parameter_ist_erlaubt() {
        let mut b = B::new();
        let ret = b.int(0);
        let f = FnDecl {
            name: "f".to_string(),
            params: vec![Param {
                name: "a".to_string(),
                ty: TypeExpr::Array { elem: Box::new(named("i32")), len: 4, span: sp() },
                span: sp(),
            }],
            ret: None,
            body: blk(Vec::new()),
            span: sp(),
            attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }]), f],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("aggregat-parameter abgelehnt:\n{}", out));
        let sig = info.fns.get("f").expect("signatur von f");
        assert_eq!(sig.params[0], Type::Array(Box::new(Type::I32), 4));
        assert_eq!(
            crate::abi::classify(&sig.params[0], &info.tcx),
            crate::abi::ArgClass::Integer(2)
        );
    }

    #[test]
    fn missing_main_is_error() {
        let prog = Program::default();
        expect_err(prog, "keine funktion 'main'");
    }

    #[test]
    fn bad_cast_is_error() {
        let mut b = B::new();
        let lit = b.int(1);
        let s = b.e(ExprKind::StructLit(
            "P".to_string(),
            vec![("x".to_string(), lit, sp())],
            sp(),
        ));
        let c = b.e(ExprKind::Cast(Box::new(s), named("i32")));
        let sd = StructDecl {
            name: "P".to_string(),
            fields: vec![("x".to_string(), named("i32"), sp())],
            span: sp(), attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(c), span: sp() }])],
            structs: vec![sd],
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "umwandlung von P nach i32 ist nicht erlaubt");
    }

    #[test]
    fn struct_literal_missing_field() {
        let mut b = B::new();
        let lit = b.int(1);
        let s = b.e(ExprKind::StructLit(
            "P".to_string(),
            vec![("x".to_string(), lit, sp())],
            sp(),
        ));
        let ret = b.int(0);
        let sd = StructDecl {
            name: "P".to_string(),
            fields: vec![
                ("x".to_string(), named("i32"), sp()),
                ("y".to_string(), named("i32"), sp()),
            ],
            span: sp(), attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let { name: "p".into(), mutable: false, ty: Some(named("P")), init: s, span: sp() },
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: vec![sd],
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "fehlt das feld 'y'");
    }

    #[test]
    fn const_is_evaluated() {
        let mut b = B::new();
        let a = b.int(6);
        let bb = b.int(7);
        let mul = b.e(ExprKind::Binary(BinOp::Mul, Box::new(a), Box::new(bb)));
        let k = b.id("K");
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(k), span: sp() }])],
            structs: Vec::new(),
            consts: vec![ConstDecl {
                name: "K".to_string(),
                ty: named("i32"),
                value: mul,
                span: sp(),
            }],
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("fehler:\n{}", out));
        assert_eq!(info.consts.get("K"), Some(&(Type::I32, 42)));
    }

    #[test]
    fn const_division_by_zero_is_error() {
        let mut b = B::new();
        let a = b.int(1);
        let z = b.int(0);
        let div = b.e(ExprKind::Binary(BinOp::Div, Box::new(a), Box::new(z)));
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: Vec::new(),
            consts: vec![ConstDecl {
                name: "K".to_string(),
                ty: named("i32"),
                value: div,
                span: sp(),
            }],
            expr_count: b.next,
        };
        expect_err(prog, "division durch null");
    }

    #[test]
    fn syscall_and_pointer_flow() {
        // var b: u8 = 65; syscall(1, 1, &b, 1); return 0
        let mut b = B::new();
        let init = b.int(65);
        let n1 = b.int(1);
        let n2 = b.int(1);
        let bid = b.id("b");
        let addr = b.e(ExprKind::Unary(UnOp::AddrOf, Box::new(bid)));
        let n3 = b.int(1);
        let sc = b.e(ExprKind::Syscall(vec![n1, n2, addr, n3]));
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let { name: "b".into(), mutable: true, ty: Some(named("u8")), init, span: sp() },
                Stmt::Expr(sc),
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("fehler:\n{}", out));
        for t in &info.expr_types {
            assert!(!t.is_error());
        }
    }

    #[test]
    fn addr_of_non_lvalue_is_error() {
        let mut b = B::new();
        let lit = b.int(1);
        let a = b.e(ExprKind::Unary(UnOp::AddrOf, Box::new(lit)));
        let p = b.e(ExprKind::Unary(UnOp::Deref, Box::new(a)));
        let prog = prog_with(&mut b, p);
        expect_err(prog, "kein zuweisbarer ausdruck");
    }

    #[test]
    fn index_must_be_usize() {
        let mut b = B::new();
        let i0 = b.int(0);
        let i1 = b.int(1);
        let i2 = b.int(2);
        let arr = b.e(ExprKind::ArrayLit(vec![i0, i1, i2]));
        let base = b.id("a");
        let iexpr = b.id("i");
        let ix = b.e(ExprKind::Index(Box::new(base), Box::new(iexpr)));
        let iinit = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "a".into(),
                    mutable: false,
                    ty: Some(TypeExpr::Array { elem: Box::new(named("i32")), len: 3, span: sp() }),
                    init: arr,
                    span: sp(),
                },
                Stmt::Let { name: "i".into(), mutable: false, ty: Some(named("i32")), init: iinit, span: sp() },
                Stmt::Return { value: Some(ix), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "index muss vom typ usize sein");
    }
}
