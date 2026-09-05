// SPDX-License-Identifier: GPL-2.0-only
//! Error unions `E!T` — SPEC §5.1 (`L7`, the normal way).
//!
//! This file belongs to the module `fehlerunionen`. It holds
//!  * the parser extensions (as `impl` on `parser::Parser`, wired up to the
//!    `// HOOK fehlerunionen` lines within `parser.rs`),
//!  * the registration of error sets and error unions at the type context,
//!  * the type check of `try`, `catch` and of the implicit conversion at
//!    `return`/`let`/assignment.
//!
//! The lowering to FIR stands at `lower_errors.rs`.
//!
//! ## Memory layout (binding)
//!
//! ```text
//! error IoError { NotFound, Permission, Closed }   // codes 1, 2, 3
//!
//! IoError            -> struct { __err: u32 }                (the code alone)
//! IoError!i32        -> struct { __err: u32, __val: i32 }     (0 = success)
//! ```
//!
//! Technically an error union is therefore an ordinary struct in
//! `types::TypeCtx`: aggregate return (`abi.rs`), System V ABI, register
//! allocation and codegen carry it without any change. The side table
//! `union_by_struct` plays the same role as `enum_by_struct` does for
//! enums.
//!
//! A `!T` value is implicitly `#[must_consume]`: the struct of the error union
//! carries `must_consume = true`, `sema::check_discard` reports discarding it.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ast::{Expr, ExprId, ExprKind, TypeExpr};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::Type;

/// Internal call name of a `try` expression. It contains `#`, so it can
/// never be an identifier out of the source text.
pub(crate) const TRY_NAME: &str = "__try#";
/// Internal call name of a `catch` expression.
pub(crate) const CATCH_NAME: &str = "__catch#";
/// Prefix of the placeholder type name between parser and type checker.
const TY_PREFIX: &str = "__eu#";

// ----------------------------------------------------------------- Data model

#[derive(Clone, Debug)]
struct ErrSet {
    name: String,
    span: Span,
    /// variant names in declaration order; code = index + 1.
    variants: Vec<String>,
    /// index into `TypeCtx::structs` (`usize::MAX` while not registered)
    struct_idx: usize,
}

/// One concrete error union `E!T`.
#[derive(Clone, Debug)]
pub(crate) struct UnionInfo {
    pub(crate) set: String,
    /// index of the struct in `types::TypeCtx`
    pub(crate) struct_idx: usize,
    pub(crate) val_ty: Type,
    pub(crate) val_off: u64,
    pub(crate) size: u64,
    pub(crate) align: u64,
}

/// Which implicit conversion is needed at a `return`/`let`/assignment place
/// (SPEC §5.1: no `ok(...)` ceremony).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoerceKind {
    /// `return value` — success case, `__err = 0`
    FromValue,
    /// `return IoError::NotFound` — error case, `__err = code`
    FromError,
}

#[derive(Clone, Debug)]
pub(crate) struct CoerceInfo {
    pub(crate) kind: CoerceKind,
    pub(crate) union: UnionInfo,
}

#[derive(Clone, Debug)]
pub(crate) struct TryInfo {
    /// Error union of the operand. The lowering does not need the error union
    /// of the surrounding function: that one is settled as the return type of
    /// the function (`Lower::sret`), and it is checked in `check_try`.
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
    /// type entries `E!T` that the parser cannot resolve yet
    pending: Vec<(String, Span, TypeExpr)>,
    unions: Vec<UnionInfo>,
    by_struct: HashMap<usize, usize>,
    /// struct index of an error set -> index into `sets`
    set_by_struct: HashMap<usize, usize>,
    /// `catch |e| …` — name of the binding per `catch` expression
    catch_bind: HashMap<ExprId, String>,
    coerce: HashMap<ExprId, CoerceInfo>,
    tries: HashMap<ExprId, TryInfo>,
    catches: HashMap<ExprId, CatchInfo>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
    /// Is `sema::collect_structs` running right now? Struct layouts are not
    /// yet computed there, so an error union over a struct could not be laid
    /// out correctly (see `hook_struct_phase`).
    static IN_STRUCTS: RefCell<bool> = const { RefCell::new(false) };
}

/// `// HOOK fehlerunionen` in `sema::collect_structs`: marks the phase in
/// which the struct layouts are not settled yet.
pub(crate) fn hook_struct_phase(active: bool) {
    IN_STRUCTS.with(|f| *f.borrow_mut() = active);
}

fn in_struct_phase() -> bool {
    IN_STRUCTS.with(|f| *f.borrow())
}

/// Does `t` contain a struct by value (pointers interrupt)?
fn contains_struct(t: &Type) -> bool {
    match t {
        Type::Struct(_) => true,
        Type::Array(e, _) => contains_struct(e),
        _ => false,
    }
}

/// Resets all registrations (one per compilation).
pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

fn set_index(name: &str) -> Option<usize> {
    REG.with(|r| r.borrow().by_name.get(name).copied())
}

/// Is this name a declared error set?
pub(crate) fn is_error_set_name(name: &str) -> bool {
    set_index(name).is_some()
}

/// Code of an error variant (from 1 upwards), if it exists.
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

/// Error union for a struct index (side table like `enum_by_struct`).
pub(crate) fn union_by_struct(idx: usize) -> Option<UnionInfo> {
    REG.with(|r| {
        let reg = r.borrow();
        reg.by_struct.get(&idx).and_then(|i| reg.unions.get(*i)).cloned()
    })
}

/// Is this the internal name of a `try` or `catch` expression?
pub(crate) fn is_result_call(name: &str) -> bool {
    name == TRY_NAME || name == CATCH_NAME
}

/// Success type of an error union (for the preview in `sema::probe_d`).
pub(crate) fn success_type(t: &Type) -> Option<Type> {
    union_of(t).map(|u| u.val_ty)
}

/// Error union for a type.
pub(crate) fn union_of(t: &Type) -> Option<UnionInfo> {
    match t {
        Type::Struct(i) => union_by_struct(*i),
        _ => None,
    }
}

/// Error set for a type (the pure error value `IoError`).
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

/// Type of an error set (the pure error value).
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

/// Name of the error binding of a `catch |e| …`.
pub(crate) fn catch_bind(id: ExprId) -> Option<String> {
    REG.with(|r| r.borrow().catch_bind.get(&id).cloned())
}

/// Name of the error set when `t` is a pure error value.
pub(crate) fn error_set_of(t: &Type) -> Option<String> {
    set_name_of(t)
}

pub(crate) fn catch_of(id: ExprId) -> Option<CatchInfo> {
    REG.with(|r| r.borrow().catches.get(&id).cloned())
}

// ------------------------------------------------------------- Parser hooks

impl<'a> Parser<'a> {
    /// `error IoError { NotFound, Permission, Closed }`
    fn errors_decl(&mut self) {
        let start = self.bump(); // 'error'
        let (name, nspan) = match self.ident("after 'error'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::LBrace, "after the name of the error set") {
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
            let (vname, vspan) = match self.ident("for a variant of the error set") {
                Some(x) => x,
                None => break,
            };
            if variants.iter().any(|v| *v == vname) {
                self.dg.error(
                    vspan,
                    format!(
                        "error variant '{}' is already declared in error set '{}'",
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
        self.close(TokKind::RBrace, "at the end of the error set");
        self.recovering = false;
        if variants.is_empty() {
            self.dg
                .error(nspan, format!("error set '{}' has no variant", name));
            return;
        }
        let span = Parser::join(start, end);
        let duplicate = REG.with(|r| {
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
        if duplicate {
            self.dg
                .error(nspan, format!("error set '{}' is already declared", name));
        }
    }
}

/// `// HOOK fehlerunionen` in `parser.rs::program` — `error` declaration.
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    if matches!(p.kind(), TokKind::KwError) {
        p.errors_decl();
        return true;
    }
    false
}

/// `// HOOK fehlerunionen` in `parser.rs::parse_type_inner` — `E!T`.
/// The name is consumed already, `sp` is its position.
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

/// ROUND 76 — THE PAYLOAD TYPE HAS TO BE VISIBLE TO THE MODULE RESOLVER TOO.
///
/// `hook_type` above does not leave the success type of `E!T` standing in
/// the syntax tree; it puts it aside into `pending` and leaves only the
/// placeholder `__eu#<n>` behind. That is what makes the two-phase
/// resolution work — but it also hides `T` from every pass that walks the
/// tree. `modules.rs::Resolver::ty` is exactly such a pass: it qualifies
/// every type name of a module with the module name, and it never saw the
/// ones inside an error union.
///
/// The consequence, found in round 76 while `lib/std/net.fi` was written:
/// a function of a MODULE could not return an error union over a struct of
/// its own module — `fn listen_tcp(..) -> NetError!Listener` reported
/// "unknown type 'Listener'", although the struct stood twenty lines above
/// it. In the root module the very same code worked, which is why the gap
/// survived seventy-five rounds: nothing outside a module had ever needed
/// it. `firnc1` never had the bug, because it renames while parsing —
/// there the payload is already qualified when it goes into `pending`.
///
/// These two functions hand the stored type expression out and take it
/// back. Out and back rather than a `&mut` on purpose: the resolver calls
/// itself while it works on the type, and a borrow held across that call
/// would be a second `borrow_mut` on the same `RefCell`.
pub(crate) fn pending_inner(name: &str) -> Option<(usize, TypeExpr)> {
    let idx: usize = name.strip_prefix(TY_PREFIX)?.parse().ok()?;
    let inner = REG.with(|r| r.borrow().pending.get(idx).map(|p| p.2.clone()))?;
    Some((idx, inner))
}

pub(crate) fn set_pending_inner(idx: usize, inner: TypeExpr) {
    REG.with(|r| {
        if let Some(p) = r.borrow_mut().pending.get_mut(idx) {
            p.2 = inner;
        }
    });
}

/// `// HOOK fehlerunionen` in `parser.rs::primary` — `try expression`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    if !matches!(p.kind(), TokKind::KwTry) {
        return None;
    }
    let start = p.bump();
    let inner = p.unary();
    let span = Parser::join(start, inner.span);
    Some(p.mk(span, ExprKind::Call(TRY_NAME.to_string(), vec![inner], span)))
}

/// `// HOOK fehlerunionen` in `parser.rs::expr` — `expression catch alt`.
/// Binds weaker than every operator and is left associative.
pub(crate) fn hook_catch(p: &mut Parser, mut lhs: Expr) -> Expr {
    while matches!(p.kind(), TokKind::KwCatch) {
        let kw = p.bump();
        // `catch |e| alt` binds the error value to `e`
        let mut bind: Option<String> = None;
        if matches!(p.kind(), TokKind::Pipe) {
            p.bump();
            match p.ident("after '|' in 'catch |e|'") {
                Some((n, _)) => bind = Some(n),
                None => return lhs,
            }
            if !p.expect(TokKind::Pipe, "after the name of the error binding") {
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

/// Assignment compatibility as in `sema.rs` (private there): equal types,
/// for pointers without regard to the `mut` marking.
///
/// **ROUND 68** — the free upcast belongs in here as well. Without it
/// `return derived` in a function of return type `AllocError!Gc[Base]` was
/// rejected although `let up: Gc[Base] = derived` right beside it was
/// allowed: the ordinary path (`sema::assignable`) knew the rule of
/// SPEC §4.4, this copy did not (docs/ROUND63.md, gap 7 — about 200 places
/// in `lib/js/` had to insert a local of the base type in between).
/// `lib/firnc1/types.fi::compatible` has had the rule since round 54; the
/// two compilers agree again only now.
fn compatible(a: &Type, b: &Type) -> bool {
    if a.is_error() || b.is_error() {
        return true;
    }
    // HOOK gc: `Gc[Derived]` -> `Gc[Base]`, and ONLY in that direction.
    if crate::gc::is_upward(a, b) {
        return true;
    }
    match (a, b) {
        (Type::Ptr { inner: x, .. }, Type::Ptr { inner: y, .. }) => compatible(x, y),
        _ => a == b,
    }
}

// ------------------------------------------- Registration at the type context

/// `// HOOK fehlerunionen` in `sema::run` (before `collect_structs`):
/// registers every error set as a struct `{ __err: u32 }`.
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
                .error(span, format!("type '{}' is already declared", name));
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

/// `// HOOK fehlerunionen` in `sema::resolve_ty_d`: resolves `E!T` and
/// creates the struct of the error union when needed.
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
            format!("unknown error set '{}'", set),
            "an error set is declared with 'error Name { A, B }'",
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
            "the success type of an error union cannot be '()'",
        );
        return Some(Type::Error);
    }
    if in_struct_phase() && contains_struct(&val_ty) {
        // While `collect_structs` runs, the struct layouts are not settled
        // yet; the error union would get a wrong size. Better a clear error
        // than a silent wrong layout (SPEC §14.1.fehlerunionen F10).
        ck.dg.error_note(
            span,
            format!(
                "an error union over the success type '{}' cannot be the field type of a struct",
                ck.tcx.name_of(&val_ty)
            ),
            "as return, variable and parameter type it is allowed; inside a struct a pointer helps",
        );
        return Some(Type::Error);
    }
    Some(get_or_create_union(ck, &set, &val_ty))
}

/// Create or reuse the error union `set!val_ty` — for modules that produce
/// an error union without it standing in the source text (`gc.rs`:
/// `gc C{…}` yields `AllocError!Gc[C]`). `None` when the error set does
/// not exist.
pub(crate) fn union_type(ck: &mut Checker, set: &str, val_ty: &Type) -> Option<Type> {
    if set_index(set).is_none() {
        return None;
    }
    Some(get_or_create_union(ck, set, val_ty))
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
            // A `!T` value is implicitly `#[must_consume]` (SPEC §5.1).
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

// --------------------------------------------------------------- Type check

/// `// HOOK fehlerunionen` in `sema::call`: `try`, `catch` and
/// `ErrorSet::Variant`. Yields `None` when it is none of those.
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
                "the error variant '{}::{}' carries no payload",
                sname, vname
            ),
        );
    }
    if variant_code(sname, vname).is_none() {
        ck.dg.error_note(
            nspan,
            format!("error set '{}' has no variant '{}'", sname, vname),
            format!("known are: {}", variant_list(sname)),
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
                        "'try' expects a value of an error union, found {}",
                        ck.tcx.name_of(&got)
                    ),
                    "an error union is made from a return type of the form 'E!T'",
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
                    "'try' is only allowed in a function with an error union return type, this one returns {}",
                    ck.tcx.name_of(&ret)
                ),
                "write the return type as 'E!T' or use 'catch'",
            );
            return inner.val_ty.clone();
        }
    };
    if rinfo.set != inner.set {
        ck.dg.error_note(
            espan,
            format!(
                "'try' yields errors of the set '{}', the function yields errors of the set '{}'",
                inner.set, rinfo.set
            ),
            "both error sets must be the same",
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
                        "'catch' expects a value of an error union on the left, found {}",
                        ck.tcx.name_of(&got)
                    ),
                    "an error union is made from a return type of the form 'E!T'",
                );
            }
            ck.type_out_expr(rhs);
            return Type::Error;
        }
    };
    let want = inner.val_ty.clone();
    // `catch |e| alt`: the error value is visible in the alternative.
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
                "the replacement value of 'catch' has type {}, expected {}",
                ck.tcx.name_of(&rt),
                ck.tcx.name_of(&want)
            ),
            "there is no implicit conversion",
        );
    }
    REG.with(|r| r.borrow_mut().catches.insert(id, CatchInfo { inner }));
    want
}

/// `// HOOK fehlerunionen` in `sema::binary`: `e == E::NotFound` compares
/// two error values of the same set. Yields `None` when it is no such
/// comparison — the ordinary check runs then.
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
                        "comparison expects two error values of the same set, found {} and {}",
                        ck.tcx.name_of(&lt),
                        ck.tcx.name_of(&rt)
                    ),
                    "there is no implicit conversion",
                );
            }
            Some(Type::Bool)
        }
    }
}

/// Type of an expression as far as it is recognizable WITHOUT a check: an
/// error variant or a variable that already carries an error set type.
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

/// `// HOOK fehlerunionen` in `sema::check_stmt` (`return`, `let`,
/// assignment): implicit conversion into an error union. Yields `false` when
/// `want` is no error union — the ordinary check runs then.
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
        return true; // already one error union — nothing to convert
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
                "error value of the set '{}' does not fit the error set '{}'",
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
            "expected {} (success value {} or an error of the set '{}'), found {}",
            ck.tcx.name_of(want),
            ck.tcx.name_of(&u.val_ty),
            u.set,
            ck.tcx.name_of(&got)
        ),
        "there is no implicit conversion",
    );
    true
}
