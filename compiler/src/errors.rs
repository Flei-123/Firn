//! Fehlerunionen `E!T` — SPEC §5.1 (`L7`, der Normalweg).
//!
//! Diese Datei gehoert dem Modul `fehlerunionen`. Sie enthaelt
//!  * die Parser-Erweiterungen (als `impl` auf `parser::Parser`, angebunden an
//!    die `// HOOK fehlerunionen`-Zeilen in `parser.rs`),
//!  * die Registrierung der Fehlermengen und Fehlerunionen im Typkontext,
//!  * die Typpruefung von `try`, `catch` und der impliziten Umwandlung bei
//!    `return`/`let`/Zuweisung.
//!
//! Das Lowering nach FIR steht in `lower_errors.rs`.
//!
//! ## Speicherlayout (verbindlich)
//!
//! ```text
//! error IoError { NotFound, Permission, Closed }   // Codes 1, 2, 3
//!
//! IoError            -> struct { __err: u32 }                (nur der Code)
//! IoError!i32        -> struct { __err: u32, __val: i32 }     (0 = Erfolg)
//! ```
//!
//! Damit ist eine Fehlerunion technisch ein gewoehnlicher Struct in
//! `types::TypeCtx`: Aggregatrueckgabe (`abi.rs`), System-V-ABI,
//! Registerzuteilung und Codegen tragen sie ohne jede Aenderung. Die
//! Seitentabelle `union_by_struct` spielt dieselbe Rolle wie `enum_by_struct`
//! fuer Aufzaehlungen.
//!
//! Ein `!T`-Wert ist implizit `#[must_consume]`: der Struct der Fehlerunion
//! traegt `must_consume = true`, `sema::check_discard` meldet das Verwerfen.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ast::{Expr, ExprId, ExprKind, TypeExpr};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::Type;

/// Interner Aufrufname eines `try`-Ausdrucks. Enthaelt `#`, kann also nie ein
/// Bezeichner aus dem Quelltext sein.
pub(crate) const TRY_NAME: &str = "__try#";
/// Interner Aufrufname eines `catch`-Ausdrucks.
pub(crate) const CATCH_NAME: &str = "__catch#";
/// Praefix des Platzhalter-Typnamens zwischen Parser und Typpruefer.
const TY_PREFIX: &str = "__eu#";

// ---------------------------------------------------------------- Datenmodell

#[derive(Clone, Debug)]
struct ErrSet {
    name: String,
    span: Span,
    /// Variantennamen in Deklarationsreihenfolge; Code = Index + 1.
    variants: Vec<String>,
    /// Index in `TypeCtx::structs` (`usize::MAX`, solange nicht angemeldet)
    struct_idx: usize,
}

/// Eine konkrete Fehlerunion `E!T`.
#[derive(Clone, Debug)]
pub(crate) struct UnionInfo {
    pub(crate) set: String,
    /// Index des Structs in `types::TypeCtx`
    pub(crate) struct_idx: usize,
    pub(crate) val_ty: Type,
    pub(crate) val_off: u64,
    pub(crate) size: u64,
    pub(crate) align: u64,
}

/// Welche implizite Umwandlung an einer `return`-/`let`-/Zuweisungsstelle
/// noetig ist (SPEC §5.1: kein `ok(...)`-Zeremoniell).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoerceKind {
    /// `return wert` — Erfolgsfall, `__err = 0`
    FromValue,
    /// `return IoError::NotFound` — Fehlerfall, `__err = code`
    FromError,
}

#[derive(Clone, Debug)]
pub(crate) struct CoerceInfo {
    pub(crate) kind: CoerceKind,
    pub(crate) union: UnionInfo,
}

#[derive(Clone, Debug)]
pub(crate) struct TryInfo {
    /// Fehlerunion des Operanden. Die Fehlerunion der umgebenden Funktion
    /// braucht das Lowering nicht: sie steht als Rueckgabetyp der Funktion
    /// fest (`Lower::sret`), geprueft wird sie in `check_try`.
    pub(crate) inner: UnionInfo,
}

#[derive(Clone, Debug)]
pub(crate) struct CatchInfo {
    pub(crate) inner: UnionInfo,
}

#[derive(Default)]
struct Registry {
    sets: Vec<ErrSet>,
    by_name: HashMap<String, usize>,
    /// Typangaben `E!T`, die der Parser noch nicht aufloesen kann
    pending: Vec<(String, Span, TypeExpr)>,
    unions: Vec<UnionInfo>,
    by_struct: HashMap<usize, usize>,
    /// Struct-Index einer Fehlermenge -> Index in `sets`
    set_by_struct: HashMap<usize, usize>,
    /// `catch |e| …` — Name der Bindung je `catch`-Ausdruck
    catch_bind: HashMap<ExprId, String>,
    coerce: HashMap<ExprId, CoerceInfo>,
    tries: HashMap<ExprId, TryInfo>,
    catches: HashMap<ExprId, CatchInfo>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
    /// Laeuft gerade `sema::collect_structs`? Dort sind Struct-Layouts noch
    /// nicht berechnet, eine Fehlerunion ueber einem Struct koennte also nicht
    /// richtig ausgelegt werden (siehe `hook_struct_phase`).
    static IN_STRUCTS: RefCell<bool> = const { RefCell::new(false) };
}

/// `// HOOK fehlerunionen` in `sema::collect_structs`: markiert die Phase, in
/// der die Struct-Layouts noch nicht feststehen.
pub(crate) fn hook_struct_phase(aktiv: bool) {
    IN_STRUCTS.with(|f| *f.borrow_mut() = aktiv);
}

fn in_struct_phase() -> bool {
    IN_STRUCTS.with(|f| *f.borrow())
}

/// Enthaelt `t` dem Wert nach einen Struct (Zeiger unterbrechen)?
fn contains_struct(t: &Type) -> bool {
    match t {
        Type::Struct(_) => true,
        Type::Array(e, _) => contains_struct(e),
        _ => false,
    }
}

/// Setzt alle Registrierungen zurueck (eine je Uebersetzung).
pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

fn set_index(name: &str) -> Option<usize> {
    REG.with(|r| r.borrow().by_name.get(name).copied())
}

/// Ist `name` eine deklarierte Fehlermenge?
pub(crate) fn is_error_set_name(name: &str) -> bool {
    set_index(name).is_some()
}

/// Code einer Fehlervariante (ab 1), sofern es sie gibt.
pub(crate) fn variant_code(set: &str, variant: &str) -> Option<i128> {
    REG.with(|r| {
        let reg = r.borrow();
        let i = *reg.by_name.get(set)?;
        let s = reg.sets.get(i)?;
        s.variants.iter().position(|v| v == variant).map(|p| p as i128 + 1)
    })
}

fn variant_list(set: &str) -> String {
    REG.with(|r| {
        let reg = r.borrow();
        match reg.by_name.get(set).and_then(|i| reg.sets.get(*i)) {
            Some(s) => s.variants.join(", "),
            None => String::new(),
        }
    })
}

/// Fehlerunion zu einem Struct-Index (Seitentabelle wie `enum_by_struct`).
pub(crate) fn union_by_struct(idx: usize) -> Option<UnionInfo> {
    REG.with(|r| {
        let reg = r.borrow();
        reg.by_struct.get(&idx).and_then(|i| reg.unions.get(*i)).cloned()
    })
}

/// Ist `name` der interne Name eines `try`- oder `catch`-Ausdrucks?
pub(crate) fn is_result_call(name: &str) -> bool {
    name == TRY_NAME || name == CATCH_NAME
}

/// Erfolgstyp einer Fehlerunion (fuer die Typvorschau in `sema::probe_d`).
pub(crate) fn success_type(t: &Type) -> Option<Type> {
    union_of(t).map(|u| u.val_ty)
}

/// Fehlerunion zu einem Typ.
pub(crate) fn union_of(t: &Type) -> Option<UnionInfo> {
    match t {
        Type::Struct(i) => union_by_struct(*i),
        _ => None,
    }
}

/// Fehlermenge zu einem Typ (der reine Fehlerwert `IoError`).
fn set_name_of(t: &Type) -> Option<String> {
    let idx = match t {
        Type::Struct(i) => *i,
        _ => return None,
    };
    REG.with(|r| {
        let reg = r.borrow();
        reg.set_by_struct
            .get(&idx)
            .and_then(|i| reg.sets.get(*i))
            .map(|s| s.name.clone())
    })
}

pub(crate) fn coerce_of(id: ExprId) -> Option<CoerceInfo> {
    REG.with(|r| r.borrow().coerce.get(&id).cloned())
}

pub(crate) fn try_of(id: ExprId) -> Option<TryInfo> {
    REG.with(|r| r.borrow().tries.get(&id).cloned())
}

/// Typ einer Fehlermenge (der reine Fehlerwert).
fn set_type(name: &str) -> Option<Type> {
    REG.with(|r| {
        let reg = r.borrow();
        let i = *reg.by_name.get(name)?;
        let s = reg.sets.get(i)?;
        if s.struct_idx == usize::MAX {
            return None;
        }
        Some(Type::Struct(s.struct_idx))
    })
}

/// Name der Fehlerbindung eines `catch |e| …`.
pub(crate) fn catch_bind(id: ExprId) -> Option<String> {
    REG.with(|r| r.borrow().catch_bind.get(&id).cloned())
}

/// Name der Fehlermenge, wenn `t` ein reiner Fehlerwert ist.
pub(crate) fn error_set_of(t: &Type) -> Option<String> {
    set_name_of(t)
}

pub(crate) fn catch_of(id: ExprId) -> Option<CatchInfo> {
    REG.with(|r| r.borrow().catches.get(&id).cloned())
}

// ------------------------------------------------------------- Parser-Hooks

impl<'a> Parser<'a> {
    /// `error IoError { NotFound, Permission, Closed }`
    fn errors_decl(&mut self) {
        let start = self.bump(); // 'error'
        let (name, nspan) = match self.ident("nach 'error'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::LBrace, "nach dem namen der fehlermenge") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let mut variants: Vec<String> = Vec::new();
        loop {
            while self.eat(&TokKind::Comma) {}
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (vname, vspan) = match self.ident("fuer eine variante der fehlermenge") {
                Some(x) => x,
                None => break,
            };
            if variants.iter().any(|v| *v == vname) {
                self.dg.error(
                    vspan,
                    format!(
                        "fehlervariante '{}' ist in fehlermenge '{}' bereits deklariert",
                        vname, name
                    ),
                );
            } else {
                variants.push(vname);
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.close(TokKind::RBrace, "am ende der fehlermenge");
        self.recovering = false;
        if variants.is_empty() {
            self.dg
                .error(nspan, format!("fehlermenge '{}' hat keine variante", name));
            return;
        }
        let span = Parser::join(start, end);
        let doppelt = REG.with(|r| {
            let mut reg = r.borrow_mut();
            if reg.by_name.contains_key(&name) {
                return true;
            }
            let i = reg.sets.len();
            reg.sets.push(ErrSet {
                name: name.clone(),
                span,
                variants,
                struct_idx: usize::MAX,
            });
            reg.by_name.insert(name.clone(), i);
            false
        });
        if doppelt {
            self.dg
                .error(nspan, format!("fehlermenge '{}' ist bereits deklariert", name));
        }
    }
}

/// `// HOOK fehlerunionen` in `parser.rs::program` — `error`-Deklaration.
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    if matches!(p.kind(), TokKind::KwError) {
        p.errors_decl();
        return true;
    }
    false
}

/// `// HOOK fehlerunionen` in `parser.rs::parse_type_inner` — `E!T`.
/// `name` ist bereits verbraucht, `sp` seine Position.
pub(crate) fn hook_type(p: &mut Parser, name: &str, sp: Span) -> Option<TypeExpr> {
    if !matches!(p.kind(), TokKind::Not) {
        return None;
    }
    let bang = p.bump();
    let inner = p.parse_type()?;
    let span = Parser::join(sp, inner.span());
    let _ = bang;
    let idx = REG.with(|r| {
        let mut reg = r.borrow_mut();
        reg.pending.push((name.to_string(), sp, inner));
        reg.pending.len() - 1
    });
    Some(TypeExpr::Named(format!("{}{}", TY_PREFIX, idx), span))
}

/// `// HOOK fehlerunionen` in `parser.rs::primary` — `try ausdruck`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    if !matches!(p.kind(), TokKind::KwTry) {
        return None;
    }
    let start = p.bump();
    let inner = p.unary();
    let span = Parser::join(start, inner.span);
    Some(p.mk(span, ExprKind::Call(TRY_NAME.to_string(), vec![inner], span)))
}

/// `// HOOK fehlerunionen` in `parser.rs::expr` — `ausdruck catch ersatzwert`.
/// Bindet schwaecher als jeder Operator und ist linksassoziativ.
pub(crate) fn hook_catch(p: &mut Parser, mut lhs: Expr) -> Expr {
    while matches!(p.kind(), TokKind::KwCatch) {
        let kw = p.bump();
        // `catch |e| ersatz` bindet den Fehlerwert an `e`
        let mut bind: Option<String> = None;
        if matches!(p.kind(), TokKind::Pipe) {
            p.bump();
            match p.ident("nach '|' in 'catch |e|'") {
                Some((n, _)) => bind = Some(n),
                None => return lhs,
            }
            if !p.expect(TokKind::Pipe, "nach dem namen der fehlerbindung") {
                return lhs;
            }
        }
        let rhs = p.or_expr();
        let span = Parser::join(lhs.span, rhs.span);
        let _ = kw;
        let e = p.mk(span, ExprKind::Call(CATCH_NAME.to_string(), vec![lhs, rhs], span));
        if let Some(n) = bind {
            REG.with(|r| r.borrow_mut().catch_bind.insert(e.id, n));
        }
        lhs = e;
    }
    lhs
}

/// Zuweisungsvertraeglichkeit wie in `sema.rs` (dort privat): gleiche Typen,
/// bei Zeigern ohne Ruecksicht auf die `mut`-Kennzeichnung.
fn compatible(a: &Type, b: &Type) -> bool {
    if a.is_error() || b.is_error() {
        return true;
    }
    match (a, b) {
        (Type::Ptr { inner: x, .. }, Type::Ptr { inner: y, .. }) => compatible(x, y),
        _ => a == b,
    }
}

// ---------------------------------------------------- Anmeldung im Typkontext

/// `// HOOK fehlerunionen` in `sema::run` (vor `collect_structs`): meldet jede
/// Fehlermenge als Struct `{ __err: u32 }` an.
pub(crate) fn declare_error_sets(ck: &mut Checker) {
    let n = REG.with(|r| r.borrow().sets.len());
    for i in 0..n {
        let (name, span) = match REG.with(|r| {
            r.borrow().sets.get(i).map(|s| (s.name.clone(), s.span))
        }) {
            Some(x) => x,
            None => continue,
        };
        if ck.tcx.lookup(&name).is_some() {
            ck.dg
                .error(span, format!("typ '{}' ist bereits deklariert", name));
            continue;
        }
        let idx = ck.tcx.declare(&name);
        ck.tcx.set_fields(idx, vec![("__err".to_string(), Type::U32)]);
        REG.with(|r| {
            let mut reg = r.borrow_mut();
            if let Some(s) = reg.sets.get_mut(i) {
                s.struct_idx = idx;
            }
            reg.set_by_struct.insert(idx, i);
        });
    }
}

/// `// HOOK fehlerunionen` in `sema::resolve_ty_d`: loest `E!T` auf und legt
/// den Struct der Fehlerunion bei Bedarf an.
pub(crate) fn hook_resolve_ty(ck: &mut Checker, te: &TypeExpr) -> Option<Type> {
    let (name, span) = match te {
        TypeExpr::Named(n, s) => (n.clone(), *s),
        _ => return None,
    };
    let idx: usize = name.strip_prefix(TY_PREFIX)?.parse().ok()?;
    let (set, set_span, inner) = REG.with(|r| r.borrow().pending.get(idx).cloned())?;
    if set_index(&set).is_none() {
        ck.dg.error_note(
            set_span,
            format!("unbekannte fehlermenge '{}'", set),
            "eine fehlermenge wird mit 'error Name { A, B }' deklariert",
        );
        return Some(Type::Error);
    }
    let val_ty = ck.resolve_ty(&inner);
    if val_ty.is_error() {
        return Some(Type::Error);
    }
    if matches!(val_ty, Type::Void) {
        ck.dg.error(
            span,
            "der erfolgstyp einer fehlerunion kann nicht '()' sein",
        );
        return Some(Type::Error);
    }
    if in_struct_phase() && contains_struct(&val_ty) {
        // Waehrend `collect_structs` stehen die Struct-Layouts noch nicht fest;
        // die Fehlerunion bekaeme eine falsche Groesse. Lieber ein klarer
        // Fehler als ein stilles Fehl-Layout (SPEC §14.1.fehlerunionen F10).
        ck.dg.error_note(
            span,
            format!(
                "eine fehlerunion ueber dem erfolgstyp '{}' kann nicht feldtyp eines structs sein",
                ck.tcx.name_of(&val_ty)
            ),
            "als rueckgabe-, variablen- und parametertyp ist sie erlaubt; im struct hilft ein zeiger",
        );
        return Some(Type::Error);
    }
    Some(get_or_create_union(ck, &set, &val_ty))
}

fn get_or_create_union(ck: &mut Checker, set: &str, val_ty: &Type) -> Type {
    let name = format!("{}!{}", set, ck.tcx.name_of(val_ty));
    if let Some(idx) = ck.tcx.lookup(&name) {
        return Type::Struct(idx);
    }
    let idx = ck.tcx.declare(&name);
    ck.tcx.set_fields(
        idx,
        vec![
            ("__err".to_string(), Type::U32),
            ("__val".to_string(), val_ty.clone()),
        ],
    );
    let (val_off, size, align) = match ck.tcx.structs.get_mut(idx) {
        Some(d) => {
            // Ein `!T`-Wert ist implizit `#[must_consume]` (SPEC §5.1).
            d.must_consume = true;
            let off = d.field("__val").map(|f| f.offset).unwrap_or(0);
            (off, d.size, d.align)
        }
        None => (0, 0, 1),
    };
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        let n = reg.unions.len();
        reg.unions.push(UnionInfo {
            set: set.to_string(),
            struct_idx: idx,
            val_ty: val_ty.clone(),
            val_off,
            size,
            align,
        });
        reg.by_struct.insert(idx, n);
    });
    Type::Struct(idx)
}

// -------------------------------------------------------------- Typpruefung

/// `// HOOK fehlerunionen` in `sema::call`: `try`, `catch` und
/// `Fehlermenge::Variante`. Liefert `None`, wenn es nichts davon ist.
pub(crate) fn hook_call(
    ck: &mut Checker,
    id: ExprId,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if name == TRY_NAME {
        return Some(check_try(ck, id, args, espan));
    }
    if name == CATCH_NAME {
        return Some(check_catch(ck, id, args));
    }
    let (sname, vname) = name.split_once("::")?;
    if !is_error_set_name(sname) {
        return None;
    }
    for a in args {
        ck.type_out_expr(a);
    }
    if !args.is_empty() {
        ck.dg.error(
            espan,
            format!(
                "die fehlervariante '{}::{}' traegt keine nutzdaten",
                sname, vname
            ),
        );
    }
    if variant_code(sname, vname).is_none() {
        ck.dg.error_note(
            nspan,
            format!("fehlermenge '{}' hat keine variante '{}'", sname, vname),
            format!("bekannt sind: {}", variant_list(sname)),
        );
        return Some(Type::Error);
    }
    let idx = REG.with(|r| {
        let reg = r.borrow();
        reg.by_name
            .get(sname)
            .and_then(|i| reg.sets.get(*i))
            .map(|s| s.struct_idx)
    });
    match idx {
        Some(i) if i != usize::MAX => Some(Type::Struct(i)),
        _ => Some(Type::Error),
    }
}

fn check_try(ck: &mut Checker, id: ExprId, args: &[Expr], espan: Span) -> Type {
    let arg = match args.first() {
        Some(a) => a,
        None => return Type::Error,
    };
    let got = ck.expr(arg, None);
    let inner = match union_of(&got) {
        Some(u) => u,
        None => {
            if !got.is_error() {
                ck.dg.error_note(
                    arg.span,
                    format!(
                        "'try' erwartet einen wert einer fehlerunion, gefunden {}",
                        ck.tcx.name_of(&got)
                    ),
                    "eine fehlerunion entsteht aus einem rueckgabetyp der form 'E!T'",
                );
            }
            return Type::Error;
        }
    };
    let ret = ck.ret.clone();
    let rinfo = match union_of(&ret) {
        Some(u) => u,
        None => {
            ck.dg.error_note(
                espan,
                format!(
                    "'try' ist nur in einer funktion mit fehlerunions-rueckgabetyp erlaubt, diese liefert {}",
                    ck.tcx.name_of(&ret)
                ),
                "schreibe den rueckgabetyp als 'E!T' oder benutze 'catch'",
            );
            return inner.val_ty.clone();
        }
    };
    if rinfo.set != inner.set {
        ck.dg.error_note(
            espan,
            format!(
                "'try' liefert fehler der menge '{}', die funktion liefert fehler der menge '{}'",
                inner.set, rinfo.set
            ),
            "beide fehlermengen muessen dieselbe sein",
        );
        return inner.val_ty.clone();
    }
    let val = inner.val_ty.clone();
    let _ = rinfo;
    REG.with(|r| r.borrow_mut().tries.insert(id, TryInfo { inner }));
    val
}

fn check_catch(ck: &mut Checker, id: ExprId, args: &[Expr]) -> Type {
    let (lhs, rhs) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a, b),
        _ => return Type::Error,
    };
    let got = ck.expr(lhs, None);
    let inner = match union_of(&got) {
        Some(u) => u,
        None => {
            if !got.is_error() {
                ck.dg.error_note(
                    lhs.span,
                    format!(
                        "'catch' erwartet links einen wert einer fehlerunion, gefunden {}",
                        ck.tcx.name_of(&got)
                    ),
                    "eine fehlerunion entsteht aus einem rueckgabetyp der form 'E!T'",
                );
            }
            ck.type_out_expr(rhs);
            return Type::Error;
        }
    };
    let want = inner.val_ty.clone();
    // `catch |e| ersatz`: der Fehlerwert ist im Ersatzausdruck sichtbar.
    let bind = catch_bind(id);
    if let Some(name) = &bind {
        let set_ty = match set_type(&inner.set) {
            Some(t) => t,
            None => Type::Error,
        };
        ck.scopes.push(std::collections::HashMap::new());
        ck.declare_var(name, set_ty, false, rhs.span);
    }
    let rt = ck.expr(rhs, Some(&want));
    if bind.is_some() {
        ck.scopes.pop();
    }
    if !rt.is_error() && !want.is_error() && !compatible(&rt, &want) {
        ck.dg.error_note(
            rhs.span,
            format!(
                "der ersatzwert von 'catch' hat typ {}, erwartet {}",
                ck.tcx.name_of(&rt),
                ck.tcx.name_of(&want)
            ),
            "es gibt keine implizite umwandlung",
        );
    }
    REG.with(|r| r.borrow_mut().catches.insert(id, CatchInfo { inner }));
    want
}

/// `// HOOK fehlerunionen` in `sema::binary`: `e == E::NotFound` vergleicht
/// zwei Fehlerwerte derselben Menge. Liefert `None`, wenn es kein solcher
/// Vergleich ist — dann laeuft die gewoehnliche Pruefung.
pub(crate) fn hook_binary(
    ck: &mut Checker,
    op: crate::ast::BinOp,
    l: &Expr,
    r: &Expr,
    espan: Span,
) -> Option<Type> {
    use crate::ast::BinOp;
    if !matches!(op, BinOp::Eq | BinOp::Ne) {
        return None;
    }
    if quiet_set(ck, l).is_none() && quiet_set(ck, r).is_none() {
        return None;
    }
    let lt = ck.expr(l, None);
    let rt = ck.expr(r, None);
    let (ls, rs) = (set_name_of(&lt), set_name_of(&rt));
    match (ls, rs) {
        (Some(a), Some(b)) if a == b => Some(Type::Bool),
        _ => {
            if !lt.is_error() && !rt.is_error() {
                ck.dg.error_note(
                    espan,
                    format!(
                        "vergleich erwartet zwei fehlerwerte derselben menge, gefunden {} und {}",
                        ck.tcx.name_of(&lt),
                        ck.tcx.name_of(&rt)
                    ),
                    "es gibt keine implizite umwandlung",
                );
            }
            Some(Type::Bool)
        }
    }
}

/// Typ eines Ausdrucks, soweit er OHNE Pruefung erkennbar ist: Fehlervariante
/// oder eine Variable, die bereits einen Fehlermengen-Typ traegt.
fn quiet_set(ck: &Checker, e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Call(name, _, _) => {
            let (set, _) = name.split_once("::")?;
            if is_error_set_name(set) {
                Some(set.to_string())
            } else {
                None
            }
        }
        ExprKind::Ident(n) => {
            let t = ck.lookup_var(n)?.ty.clone();
            set_name_of(&t)
        }
        _ => None,
    }
}

/// `// HOOK fehlerunionen` in `sema::check_stmt` (`return`, `let`, Zuweisung):
/// implizite Umwandlung in eine Fehlerunion. Liefert `false`, wenn `want`
/// keine Fehlerunion ist — dann laeuft die gewoehnliche Pruefung.
pub(crate) fn hook_coerce(ck: &mut Checker, e: &Expr, want: &Type) -> bool {
    let u = match union_of(want) {
        Some(u) => u,
        None => return false,
    };
    let got = ck.expr(e, Some(&u.val_ty));
    if got.is_error() {
        return true;
    }
    if got == *want {
        return true; // schon eine Fehlerunion — nichts umzuwandeln
    }
    if let Some(set) = set_name_of(&got) {
        if set == u.set {
            REG.with(|r| {
                r.borrow_mut()
                    .coerce
                    .insert(e.id, CoerceInfo { kind: CoerceKind::FromError, union: u.clone() })
            });
            return true;
        }
        ck.dg.error(
            e.span,
            format!(
                "fehlerwert der menge '{}' passt nicht zur fehlermenge '{}'",
                set, u.set
            ),
        );
        return true;
    }
    if compatible(&got, &u.val_ty) {
        REG.with(|r| {
            r.borrow_mut()
                .coerce
                .insert(e.id, CoerceInfo { kind: CoerceKind::FromValue, union: u.clone() })
        });
        return true;
    }
    ck.dg.error_note(
        e.span,
        format!(
            "erwartet {} (erfolgswert {} oder ein fehler der menge '{}'), gefunden {}",
            ck.tcx.name_of(want),
            ck.tcx.name_of(&u.val_ty),
            u.set,
            ck.tcx.name_of(&got)
        ),
        "es gibt keine implizite umwandlung",
    );
    true
}
