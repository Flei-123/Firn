// SPDX-License-Identifier: GPL-2.0-only
//! `interface` and **dynamic dispatch** — round 46 (SPEC §6.2).
//!
//! ## What this round adds
//!
//! Round 45 brought `impl T { fn m(*self) … }` — a pure writing aid:
//! `x.m(a)` became `T__m(&x, a)`, decided by the **static** type alone.
//! This round adds the other case to that, the one SPEC §6.2 demands
//! explicitly: **one call site, many types**.
//!
//! ```text
//! interface Area {                 // contract: methods without a body
//!     fn area(*self) -> i64
//!     fn scale(*mut self, k: i64)
//! }
//!
//! impl Area for Rectangle { … }    // implementation, fully checked
//!
//! let f: dyn Area = (&r) as dyn Area         // interface value
//! f.area()                                   // through the method table
//! ```
//!
//! ## The representation — binding
//!
//! ```text
//! interface I      -> struct "dyn I" at types::TypeCtx, 16 bytes:
//!                       data:  *mut u8   (offset 0) — the value itself
//!                       table: *mut u8   (offset 8) — the method table
//! dyn I            -> exactly that struct (a FAT POINTER, no pointer)
//! impl I for T     -> method table `.L__iface.I.T` at `.rodata`:
//!                       .quad T__m1
//!                       .quad T__m2      (order = order within `I`)
//! ```
//!
//! The internal struct name carries a **space** (`"dyn I"`) — the same build
//! as `"gc C"` in `gc.rs` and `"method m"` in `impls.rs`. It cannot arise
//! from any identifier of the source text, and `TypeCtx::name_of` prints it
//! unchanged: every error message shows `dyn I`, exactly as it is written
//! in the source text.
//!
//! ## Why a struct and no `Type` of its own
//!
//! A `Type::Dyn(..)` would have touched every case split over `Type` —
//! layout, ABI, monomorphization, optimizer, debug info. As a struct the
//! interface value is an **ordinary 16-byte aggregate**: `abi::classify`
//! gives it two INTEGER words, and it is copied, passed, returned and put
//! down in the frame like every other struct. The whole language change
//! sits in the parser, the type checker and two FIR instructions.
//!
//! ## Why the collector still finds the value behind it
//!
//! The data pointer is an **ordinary pointer to the start** of the value —
//! not veiled (unlike `GcWeak`, `gc.rs`), not offset onto a field. A `dyn I`
//! sits in the frame or in a callee-saved register; the collector searches
//! both conservatively (SPEC §3.5.3), and during the collection run
//! `Op::GcAddr { regs: true }` rescues the registers into the state block
//! beforehand. An interface value thus keeps its object alive even when
//! there is no other root left (`tests/823_iface_gc_core.fi`).
//!
//! The **one** place where that does not carry is the heap: there the
//! collector traces PRECISELY along the field layout and would not know
//! about the data pointer in a `dyn I` field. That is why `dyn I` as a field
//! of a `gc class` is an error (`iface_dyn_in_gc_class.fi`) — a hole in a
//! guarantee would be worse than a missing convenience.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::ast::{Expr, TypeExpr};
use crate::diag::{Diags, Span};
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::{Type, TypeCtx};

/// Prefix of the internal struct name of an interface value.
/// The space makes it unreachable from the source text.
pub(crate) const P_DYN: &str = "dyn ";
/// Prefix of the method tables in the assembler (file local, `.L`).
const TABLE_LABEL: &str = ".L__iface.";
/// Offset of the data pointer in the interface value.
pub(crate) const OFF_DATA: u64 = 0;
/// Offset of the method table in the interface value.
pub(crate) const OFF_TABLE: u64 = 8;

// ----------------------------------------------------------------- Data model

#[derive(Clone, Debug)]
struct Method {
    name: String,
    /// parameters WITHOUT the receiver.
    params: Vec<TypeExpr>,
    ret: Option<TypeExpr>,
    /// `*mut self` instead of `*self` — intent today, no constraint (impls.rs).
    mutable: bool,
    span: Span,
    /// after `hook_check_impls`: resolved types
    ptypes: Vec<Type>,
    rtyp: Type,
    /// types already resolved (addenda from `comptime` would else report twice)
    resolved: bool,
    /// The signature mentions `Self` (round 50). Then `ptypes`/`rtyp` hang off
    /// the implementing type and are resolved ONLY PER IMPLEMENTATION; globally
    /// they stay empty, and through `dyn I` the method is not callable.
    has_self: bool,
}

#[derive(Clone, Debug)]
struct Interface {
    name: String,
    methods: Vec<Method>,
    /// index of the struct `"dyn I"` in `TypeCtx`, `usize::MAX` until registered
    struct_idx: usize,
}

#[derive(Clone, Debug)]
struct Impl {
    iface: String,
    /// type name as it stands in the source (BEFORE the module renaming).
    ty: String,
    span: Span,
    /// after the check: index of the struct, `usize::MAX` = not resolved
    struct_idx: usize,
    /// after the check: the name under which the methods of this type stand
    /// (`T__m`) — that is, the FINAL name after the module renaming, without
    /// the internal `"gc "` of a class. The code generator no longer holds the
    /// type table; that is why it stands here.
    prefix: String,
    /// after the check: `true` when the implementation is complete
    ok: bool,
    /// already checked (addenda from `comptime` would else report twice)
    checked: bool,
}

#[derive(Default)]
struct Registry {
    ifaces: Vec<Interface>,
    impls: Vec<Impl>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

/// Resets the registry (one per compilation, `parser::reset_hooks`).
pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

fn iface_index(name: &str) -> Option<usize> {
    REG.with(|r| r.borrow().ifaces.iter().position(|i| i.name == name))
}

/// Is there any interface at all in this compilation?
pub(crate) fn has_interfaces() -> bool {
    REG.with(|r| !r.borrow().ifaces.is_empty())
}

/// Is `sname` the internal name of an interface value? Yields the
/// interface name.
pub(crate) fn interface_of(sname: &str) -> Option<&str> {
    sname.strip_prefix(P_DYN)
}

/// Name of the struct behind `dyn I`.
fn dyn_name(iface: &str) -> String {
    format!("{}{}", P_DYN, iface)
}

/// Is this type an interface value?
pub(crate) fn is_dyn(tcx: &TypeCtx, t: &Type) -> bool {
    match t {
        Type::Struct(i) => tcx
            .structs
            .get(*i)
            .map(|s| s.name.starts_with(P_DYN))
            .unwrap_or(false),
        _ => false,
    }
}

/// Key of the method table: `<interface>.<type>`.
fn table_key(iface: &str, ty_name: &str) -> String {
    format!("{}.{}", iface, ty_name)
}

/// The builtin type behind a name — `None` when it is not one.
///
/// Since round 50 a BASE TYPE may implement interfaces as well
/// (`impl Ord for i32`). Without that, `vec_sort[T: Ord]` could only have
/// sorted structs, and the standard library would have had to keep the
/// hard wired comparison.
pub(crate) fn base_ty_of_name(n: &str) -> Option<Type> {
    // ROUND 70: `impl Ord for int` means `impl Ord for i32` -- the name is
    // folded onto the canonical spelling first (types.rs::canon_name), so
    // that only ONE method name (`i32__less`) can ever come into being.
    Some(match crate::types::canon_name(n) {
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
        "f64" => Type::F64,
        "f32" => Type::F32,
        _ => return None,
    })
}

/// Name of a base type for the method naming scheme (`i32__less`).
pub(crate) fn base_ty_name(t: &Type) -> Option<&'static str> {
    Some(match t {
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
        Type::Bool => "bool",
        Type::F64 => "f64",
        Type::F32 => "f32",
        _ => return None,
    })
}

/// Is this the name of a method on a BASE TYPE (`i32__less`)?
///
/// Such methods hold PROGRAM WIDE and are not renamed by `modules.rs` —
/// just like interfaces, gc classes and generic templates. The reason is
/// the same: the type `i32` belongs to no module. If the method became
/// `vec__i32__less`, the resolution would keep looking for `i32__less`
/// and find nothing.
pub(crate) fn is_base_ty_method(name: &str) -> bool {
    match name.split_once(crate::impls::SEP) {
        Some((header, rest)) => !rest.is_empty() && base_ty_of_name(header).is_some(),
        None => false,
    }
}

/// Does this type expression mention `Self`?
fn names_self(te: &TypeExpr) -> bool {
    match te {
        TypeExpr::Named(n, _) => n == "Self",
        TypeExpr::Ptr { inner, .. } => names_self(inner),
        TypeExpr::Array { elem, .. } => names_self(elem),
        // Round 58: `Self` may hide in the signature of a function value.
        TypeExpr::Fn { params, ret, .. } => {
            params.iter().any(names_self) || ret.as_ref().is_some_and(|t| names_self(t))
        }
    }
}

// ---------------------------------------------------- Bounds (round 50)
//
// `fn f[T: Ord](…)` — the bound is checked at the INSTANTIATION
// (`mono.rs::bind_params`), so before the type checker runs. At that point
// there is neither a struct table nor resolved types; what there is are
// the names: the registry in this file and the list of all function names
// of the merged program. The message is built from exactly that, and
// exactly for that reason it can name the METHOD THAT IS MISSING instead
// of striking later as "unknown method" in the middle of some
// instantiated function.

/// Readable form of a type not yet resolved (for messages before the type
/// checker). `Self` stays `Self` — exactly as it stands in the interface.
fn te_text(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Named(n, _) => n.clone(),
        TypeExpr::Ptr { mutable, inner, .. } => {
            format!("*{}{}", if *mutable { "mut " } else { "" }, te_text(inner))
        }
        TypeExpr::Array { elem, len, .. } => format!("[{}; {}]", te_text(elem), len),
        TypeExpr::Fn { params, ret, .. } => {
            let ps: Vec<String> = params.iter().map(te_text).collect();
            match ret {
                Some(t) => format!("fn({}) -> {}", ps.join(", "), te_text(t)),
                None => format!("fn({})", ps.join(", ")),
            }
        }
    }
}

/// Signature of an interface method from the UNRESOLVED head.
/// (`signature` further down does the same with resolved types; here no
/// type checker has run yet.)
fn header_signature(m: &Method) -> String {
    let mut s = String::from(if m.mutable { "*mut self" } else { "*self" });
    for t in &m.params {
        s.push_str(", ");
        s.push_str(&te_text(t));
    }
    match &m.ret {
        None => format!("fn {}({})", m.name, s),
        Some(r) => format!("fn {}({}) -> {}", m.name, s, te_text(r)),
    }
}

/// Does the type `ty_name` implement the interface `iface`?
///
/// NAMES are compared, not types — the type checker does not exist yet at
/// this point. The name in the registry stands as it was written in the
/// source; the type argument on the other hand already carries the name
/// after the module renaming. Hence the same three steps as in `ty_struct`:
/// equal, `gc <name>`, or ending in `__<name>` (a type from a module).
fn impl_da(iface: &str, ty_name: &str) -> bool {
    REG.with(|r| {
        r.borrow().impls.iter().any(|u| {
            if u.iface != iface {
                return false;
            }
            if u.ty == ty_name || ty_name == format!("gc {}", u.ty) {
                return true;
            }
            // The suffix rule holds for named types from a module ONLY.
            // For a base type it would be wrong: `Vec__i32` ends in
            // `__i32`, but is the struct `Vec[i32]` and not `i32`.
            base_ty_of_name(&u.ty).is_none()
                && ty_name.ends_with(&format!("__{}", u.ty))
        })
    })
}

/// `// HOOK iface` in `mono.rs::bound_ok` — `T: I` at the instantiation.
/// `true` = the bound is satisfied.
pub(crate) fn bound_check(
    dg: &mut Diags,
    fnames: &HashSet<String>,
    arg: &TypeExpr,
    iface: &str,
    pname: &str,
    base: &str,
    span: Span,
) -> bool {
    let ii = match iface_index(iface) {
        Some(i) => i,
        None => {
            let known: Vec<String> =
                REG.with(|r| r.borrow().ifaces.iter().map(|s| s.name.clone()).collect());
            let note = if known.is_empty() {
                "no interface is declared in this compilation; \
                 built in are only Any, Int and Scalar"
                    .to_string()
            } else {
                format!("known are: {} (built in: Any, Int, Scalar)", known.join(", "))
            };
            dg.error_note(
                span,
                format!(
                    "unknown interface '{}' as bound on the type parameter '{}' of '{}'",
                    iface, pname, base
                ),
                note,
            );
            return false;
        }
    };
    // An interface is implemented by a NAMED type. A pointer or a field
    // has no name under which an implementation could stand.
    let ty_name = match arg {
        TypeExpr::Named(n, _) => n.clone(),
        _ => {
            dg.error_note(
                span,
                format!(
                    "type argument '{}' does not satisfy the bound '{}' of the type parameter '{}' of '{}'",
                    te_text(arg), iface, pname, base
                ),
                format!(
                    "an interface is implemented with 'impl {} for <type>'; \
                     a pointer or field type has no name under which that could stand",
                    iface
                ),
            );
            return false;
        }
    };
    if impl_da(iface, &ty_name) {
        return true;
    }
    // No `impl I for T`. Now the useful message: which methods of the
    // interface does the type have at all?
    let methods = REG.with(|r| r.borrow().ifaces[ii].methods.clone());
    let mut missing: Vec<String> = Vec::new();
    for m in &methods {
        let full = format!("{}{}{}", ty_name, crate::impls::SEP, m.name);
        if !fnames.contains(&full) {
            missing.push(header_signature(m));
        }
    }
    let note = if methods.is_empty() {
        format!("'{}' has no method; only 'impl {} for {}' is missing", iface, iface, ty_name)
    } else if missing.is_empty() {
        format!(
            "'{}' has all methods of '{}'; the block 'impl {} for {} {{ … }}' is missing",
            ty_name, iface, iface, ty_name
        )
    } else {
        format!(
            "{} is missing in 'impl {} for {} {{ … }}'",
            missing
                .iter()
                .map(|x| format!("'{}'", x))
                .collect::<Vec<_>>()
                .join(" and "),
            iface,
            ty_name
        )
    };
    dg.error_note(
        span,
        format!(
            "type '{}' does not implement the interface '{}' — bound on the type parameter '{}' of '{}'",
            ty_name, iface, pname, base
        ),
        note,
    );
    false
}

// ------------------------------------------------------------------- Parser

/// `// HOOK iface` in `parser.rs::program` — `interface I { … }`.
///
/// `interface` is NO keyword (the tokenizer does not know it) but an
/// identifier in a position where nothing else may stand — the same
/// solution as `gc class` (gc.rs) and `impl` (impls.rs). That keeps
/// `interface` valid as a variable name.
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
            "no attribute is allowed before 'interface'".to_string(),
            "attributes on interfaces do not exist in this stage".to_string(),
        );
        p.pending_attrs.clear();
    }
    let (name, nsp) = match p.ident("after 'interface'") {
        Some(x) => x,
        None => {
            p.recovering = false;
            p.sync_item();
            return;
        }
    };
    if !p.expect(TokKind::LBrace, "after the name of the interface") {
        p.recovering = false;
        p.sync_item();
        return;
    }
    let mut methods: Vec<Method> = Vec::new();
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
                "expected 'fn' in an interface, found '{}'",
                p.kind().text()
            ));
            p.recovering = false;
            p.sync_item();
            return;
        }
        // One broken method aborts the WHOLE interface — the same rule as in
        // the `impl` block (impls.rs): the first message is the only one that
        // explains anything.
        match method_head(p, &name) {
            Some(m) => {
                if methods.iter().any(|x| x.name == m.name) {
                    p.dg.error(
                        m.span,
                        format!(
                            "the interface '{}' already has the method '{}'",
                            name, m.name
                        ),
                    );
                }
                methods.push(m);
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
    p.close(TokKind::RBrace, "at the end of the interface");
    p.recovering = false;
    if iface_index(&name).is_some() {
        p.dg.error(
            nsp,
            format!("the interface '{}' is already declared", name),
        );
        return;
    }
    REG.with(|r| {
        r.borrow_mut().ifaces.push(Interface {
            name,
            methods,
            struct_idx: usize::MAX,
        })
    });
}

/// One method of the interface: `fn m(<receiver>[, param…]) [-> T]`
/// — without a body.
fn method_head(p: &mut Parser, iface: &str) -> Option<Method> {
    p.bump(); // 'fn'
    let (name, nsp) = match p.ident("after 'fn' in an interface") {
        Some(x) => x,
        None => return None,
    };
    if p.at(&TokKind::LBracket) {
        p.error_here("an interface method cannot be generic in this stage");
        return None;
    }
    if !p.expect(TokKind::LParen, "after the method name") {
        return None;
    }
    // THE RECEIVER MUST BE A POINTER. Through the method table only the data
    // pointer is available — a copy of the value is something the caller
    // could not build at all, it does not know the concrete type.
    let mutable = if p.at(&TokKind::Star) {
        match crate::impls::ptr_self(p) {
            Some((m, _)) => m,
            None => {
                p.error_here(format!(
                    "the receiver of an interface method is '*self' or '*mut self' ('{}.{}')",
                    iface, name
                ));
                return None;
            }
        }
    } else {
        p.error_here(format!(
            "the receiver of an interface method is '*self' or '*mut self' ('{}.{}')",
            iface, name
        ));
        return None;
    };
    let mut params: Vec<TypeExpr> = Vec::new();
    if p.eat(&TokKind::Comma) {
        params = p.params().into_iter().map(|x| x.ty).collect();
    }
    p.close(TokKind::RParen, "after the parameter list");
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
            "an interface method has no body ('{}.{}')",
            iface, name
        ));
        return None;
    }
    // `Self` (round 50): the type that implements the interface. Only with it
    // can an ordering be written down — `fn less(*self, b: *Self)`.
    // Without `Self` a CONCRETE type would have to stand in the interface, and
    // a general `Ord` would be impossible.
    let has_self =
        params.iter().any(names_self) || ret.as_ref().map(names_self).unwrap_or(false);
    Some(Method {
        name,
        params,
        ret,
        mutable,
        span: nsp,
        ptypes: Vec::new(),
        rtyp: Type::Void,
        resolved: false,
        has_self,
    })
}

/// `// HOOK iface` in `parser.rs::parse_type_inner` — `dyn I`.
///
/// The type name is drawn together into ONE name with a space; it is
/// resolved in the type checker, which knows the struct table.
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

/// `// HOOK iface` in `impls.rs::impl_decl` — register `impl I for T { … }`.
pub(crate) fn remember_impl(iface: String, ty: String, span: Span) {
    REG.with(|r| {
        r.borrow_mut().impls.push(Impl {
            iface,
            ty,
            span,
            struct_idx: usize::MAX,
            prefix: String::new(),
            ok: false,
            checked: false,
        })
    });
}

// --------------------------------------------------------------- Type check

/// `// HOOK iface` in `sema::add_items_inner` (BEFORE `collect_structs`):
/// create the struct `"dyn I"` for every interface.
///
/// Being called twice is allowed (`comptime` addenda): an interface that is
/// already registered is skipped.
pub(crate) fn declare_interfaces(ck: &mut Checker) {
    let n = REG.with(|r| r.borrow().ifaces.len());
    for i in 0..n {
        let (name, done) = REG.with(|r| {
            let reg = r.borrow();
            let s = &reg.ifaces[i];
            (s.name.clone(), s.struct_idx != usize::MAX)
        });
        if done {
            continue;
        }
        let sidx = ck.tcx.declare(&dyn_name(&name));
        ck.tcx.set_fields(
            sidx,
            vec![
                ("data".to_string(), Type::ptr(Type::U8, true)),
                ("table".to_string(), Type::ptr(Type::U8, true)),
            ],
        );
        REG.with(|r| r.borrow_mut().ifaces[i].struct_idx = sidx);
    }
}

/// `// HOOK iface` in `sema::resolve_ty_d` — `dyn I` with unknown `I`.
pub(crate) fn hook_resolve_ty(ck: &mut Checker, te: &TypeExpr) -> Option<Type> {
    let (name, span) = match te {
        TypeExpr::Named(n, s) => (n.as_str(), *s),
        _ => return None,
    };
    let iface = interface_of(name)?;
    if ck.tcx.lookup(name).is_some() {
        return None; // the ordinary resolution finds the struct
    }
    ck.dg.error_note(
        span,
        format!("unknown interface '{}'", iface),
        format!("an interface is declared with 'interface {} {{ … }}'", iface),
    );
    Some(Type::Error)
}

/// The name under which the methods of a type stand: `T__m`.
/// For a `gc class` that is the class name WITHOUT the internal `"gc "`.
pub(crate) fn method_prefix(tcx: &TypeCtx, idx: usize) -> String {
    match tcx.structs.get(idx) {
        Some(s) => s.name.strip_prefix("gc ").unwrap_or(&s.name).to_string(),
        None => String::new(),
    }
}

/// Comparison of two types at a signature boundary — the same rule as
/// `sema::compatible`: pointers are compared WITHOUT the mutability
/// (`*T` and `*mut T` can be used for each other in this language).
fn fits(a: &Type, b: &Type) -> bool {
    if a.is_error() || b.is_error() {
        return true;
    }
    match (a, b) {
        (Type::Ptr { inner: x, .. }, Type::Ptr { inner: y, .. }) => fits(x, y),
        _ => a == b,
    }
}

/// Struct behind the type name of an implementation.
///
/// The name stands in the registry as it was written in the source text —
/// the module renaming (`modules.rs`) runs AFTER parsing and does not touch
/// the registry. That is why the search happens here in three steps: the
/// name itself, the `gc class` of the same name, and finally EXACTLY ONE
/// struct whose name ends in `__<name>` (that is the case "type in a
/// module"). Several hits are an error — guessing would be the more
/// dangerous choice.
fn ty_struct(ck: &Checker, name: &str) -> Result<usize, bool> {
    if let Some(i) = ck.tcx.lookup(name) {
        return Ok(i);
    }
    if let Some(i) = ck.tcx.lookup(&format!("gc {}", name)) {
        return Ok(i);
    }
    let suffix = format!("__{}", name);
    let hit: Vec<usize> = ck
        .tcx
        .structs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.ends_with(&suffix))
        .map(|(i, _)| i)
        .collect();
    match hit.len() {
        1 => Ok(hit[0]),
        0 => Err(false),
        _ => Err(true),
    }
}

/// Readable signature of one interface method (for the error message).
///
/// The types are passed explicitly: with a signature containing `Self` they
/// do not stand in the interface but hang off the implementation.
fn signature(ck: &Checker, m: &Method, ptypes: &[Type], rtyp: &Type) -> String {
    let mut s = String::from(if m.mutable { "*mut self" } else { "*self" });
    for t in ptypes {
        s.push_str(", ");
        s.push_str(&ck.tcx.name_of(t));
    }
    match rtyp {
        Type::Void => format!("fn {}({})", m.name, s),
        r => format!("fn {}({}) -> {}", m.name, s, ck.tcx.name_of(r)),
    }
}

/// Resolves a type of the interface, substituting `Self` along the way.
fn resolve_with_self(ck: &mut Checker, te: &TypeExpr, slf: &Type) -> Type {
    match te {
        TypeExpr::Named(n, _) if n == "Self" => slf.clone(),
        TypeExpr::Ptr { mutable, inner, .. } => {
            Type::ptr(resolve_with_self(ck, inner, slf), *mutable)
        }
        TypeExpr::Array { elem, len, .. } => {
            Type::Array(Box::new(resolve_with_self(ck, elem, slf)), *len)
        }
        _ => ck.resolve_ty(te),
    }
}

/// `// HOOK iface` in `sema::add_items_inner` (AFTER `collect_fns`):
/// checks every implementation fully — all methods present, all signatures fit.
pub(crate) fn hook_check_impls(ck: &mut Checker) {
    // 1. Resolve the types of the interface methods (once per method).
    let n = REG.with(|r| r.borrow().ifaces.len());
    for i in 0..n {
        let cnt = REG.with(|r| r.borrow().ifaces[i].methods.len());
        for k in 0..cnt {
            let (done, params, ret, has_self) = REG.with(|r| {
                let reg = r.borrow();
                let m = &reg.ifaces[i].methods[k];
                (m.resolved, m.params.clone(), m.ret.clone(), m.has_self)
            });
            if done {
                continue;
            }
            // A signature containing `Self` has NO types globally — it gets
            // them only per implementation (`check_impl_am`). Resolving here
            // would mean looking for `Self` as an ordinary type name, and
            // that would be a message about a type that nobody ever wanted
            // to declare.
            if has_self {
                REG.with(|r| r.borrow_mut().ifaces[i].methods[k].resolved = true);
                continue;
            }
            let pt: Vec<Type> = params.iter().map(|t| ck.resolve_ty(t)).collect();
            let rt = match &ret {
                Some(t) => ck.resolve_ty(t),
                None => Type::Void,
            };
            REG.with(|r| {
                let mut reg = r.borrow_mut();
                let m = &mut reg.ifaces[i].methods[k];
                m.ptypes = pt;
                m.rtyp = rt;
                m.resolved = true;
            });
        }
    }
    // 2. Check every implementation.
    let cnt = REG.with(|r| r.borrow().impls.len());
    for u in 0..cnt {
        let (iface, ty, span, checked) = REG.with(|r| {
            let reg = r.borrow();
            let x = &reg.impls[u];
            (x.iface.clone(), x.ty.clone(), x.span, x.checked)
        });
        if checked {
            continue;
        }
        REG.with(|r| r.borrow_mut().impls[u].checked = true);
        check_impl(ck, u, &iface, &ty, span);
    }
    // 3. No interface value in the heap: the collector traces PRECISELY
    //    there (SPEC §3.5.3) and does not know the data pointer in a `dyn I`.
    check_gc_fields(ck);
}

fn check_impl(ck: &mut Checker, u: usize, iface: &str, ty: &str, span: Span) {
    let ii = match iface_index(iface) {
        Some(i) => i,
        None => {
            let known: Vec<String> =
                REG.with(|r| r.borrow().ifaces.iter().map(|s| s.name.clone()).collect());
            let note = if known.is_empty() {
                "no interface is declared in this compilation".to_string()
            } else {
                format!("known are: {}", known.join(", "))
            };
            ck.dg.error_note(
                span,
                format!("unknown interface '{}'", iface),
                note,
            );
            return;
        }
    };
    // CARRIER OF THE IMPLEMENTATION: a struct or — since round 50 — a builtin
    // BASE TYPE. A base type does not stand at the struct table; `struct_idx`
    // then stays `usize::MAX`, and everything that needs this index (method
    // table, `as dyn I`) does not hold for it. Dynamic dispatch over a base
    // type is thereby ruled out, static dispatch is not — and exactly that is
    // what is needed (`vec_sort[i32]`).
    // BASE TYPE FIRST. `i32` always means the builtin type; the suffix
    // search in `ty_struct` (third field: "exactly one struct whose name
    // ends in `__<name>`") would otherwise find `Vec__i32` — which is
    // `Vec[i32]` and not `i32`.
    if let Some(t) = base_ty_of_name(ty) {
        return check_impl_am(ck, u, iface, ii, ty, span, usize::MAX, t);
    }
    let (sidx, self_ty) = match ty_struct(ck, ty) {
        Ok(i) => (i, Type::Struct(i)),
        Err(true) => {
            ck.dg.error_note(
                span,
                format!("the type '{}' is ambiguous", ty),
                "several modules declare a type of this name".to_string(),
            );
            return;
        }
        Err(false) => {
            ck.dg.error(span, format!("unknown type '{}'", ty));
            return;
        }
    };
    check_impl_am(ck, u, iface, ii, ty, span, sidx, self_ty)
}

/// The second part: check the implementation against a KNOWN carrier.
/// `sidx == usize::MAX` means "base type" — then there is no struct and
/// therefore neither a method table nor `as dyn I`.
fn check_impl_am(
    ck: &mut Checker,
    u: usize,
    iface: &str,
    ii: usize,
    ty: &str,
    span: Span,
    sidx: usize,
    self_ty: Type,
) {
    if sidx != usize::MAX && ck.tcx.structs[sidx].name.starts_with(P_DYN) {
        ck.dg.error_note(
            span,
            format!("'{}' is an interface and not a type", ty),
            "an interface does not implement an interface".to_string(),
        );
        return;
    }
    let prefix = if sidx == usize::MAX {
        ty.to_string()
    } else {
        method_prefix(&ck.tcx, sidx)
    };
    let display = if sidx == usize::MAX {
        ty.to_string()
    } else {
        ck.tcx.structs[sidx].name.clone()
    };
    // Implemented twice — what is compared is the METHOD PREFIX of the
    // resolved carrier, so that `impl I for T` and `impl I for module.T` are
    // recognized as the same one and base types count along. (An empty
    // prefix means: that implementation was faulty already.)
    let duplicate = REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .take(u)
            .any(|x| x.iface == iface && !x.prefix.is_empty() && x.prefix == prefix)
    });
    if duplicate {
        ck.dg.error(
            span,
            format!("'{}' already implements the interface '{}'", ty, iface),
        );
        return;
    }
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        reg.impls[u].struct_idx = sidx;
        reg.impls[u].prefix = prefix.clone();
    });
    let methods = REG.with(|r| r.borrow().ifaces[ii].methods.clone());
    let mut complete = true;
    for m in &methods {
        // With `Self` the types hang off THIS implementation, not off the
        // interface — which is why they are resolved here and not in step 1.
        let (ptypes, rtyp): (Vec<Type>, Type) = if m.has_self {
            (
                m.params
                    .iter()
                    .map(|t| resolve_with_self(ck, t, &self_ty))
                    .collect(),
                match &m.ret {
                    Some(t) => resolve_with_self(ck, t, &self_ty),
                    None => Type::Void,
                },
            )
        } else {
            (m.ptypes.clone(), m.rtyp.clone())
        };
        let full = format!("{}{}{}", prefix, crate::impls::SEP, m.name);
        let sig = match ck.fns.get(&full) {
            Some(s) => s.clone(),
            None => {
                complete = false;
                ck.dg.error_note(
                    span,
                    format!(
                        "'{}' does not implement the method '{}.{}'",
                        display, iface, m.name
                    ),
                    format!("'{}' is expected in the block", signature(ck, m, &ptypes, &rtyp)),
                );
                continue;
            }
        };
        // Receiver: a pointer to EXACTLY this type.
        let recv_ok = match sig.params.first() {
            Some(Type::Ptr { inner, .. }) => **inner == self_ty,
            _ => false,
        };
        if !recv_ok {
            complete = false;
            ck.dg.error_note(
                span,
                format!(
                    "the receiver of '{}.{}' does not fit '{}'",
                    display, m.name, iface
                ),
                format!("'{}' is expected in the block", signature(ck, m, &ptypes, &rtyp)),
            );
            continue;
        }
        if sig.params.len() != ptypes.len() + 1 {
            complete = false;
            ck.dg.error_note(
                span,
                format!(
                    "'{}.{}' has {} parameters, the interface '{}' requires {}",
                    display,
                    m.name,
                    sig.params.len() - 1,
                    iface,
                    ptypes.len()
                ),
                format!("'{}' is expected in the block", signature(ck, m, &ptypes, &rtyp)),
            );
            continue;
        }
        let mut matching = true;
        for (k, expected) in ptypes.iter().enumerate() {
            let actual = &sig.params[k + 1];
            if !fits(actual, expected) {
                matching = false;
                ck.dg.error_note(
                    span,
                    format!(
                        "parameter {} of '{}.{}' has type {}, the interface '{}' requires {}",
                        k + 1,
                        display,
                        m.name,
                        ck.tcx.name_of(actual),
                        iface,
                        ck.tcx.name_of(expected)
                    ),
                    format!("'{}' is expected in the block", signature(ck, m, &ptypes, &rtyp)),
                );
                break;
            }
        }
        if !matching {
            complete = false;
            continue;
        }
        if !fits(&sig.ret, &rtyp) {
            complete = false;
            ck.dg.error_note(
                span,
                format!(
                    "'{}.{}' returns {}, the interface '{}' requires {}",
                    display,
                    m.name,
                    ck.tcx.name_of(&sig.ret),
                    iface,
                    ck.tcx.name_of(&rtyp)
                ),
                format!("'{}' is expected in the block", signature(ck, m, &ptypes, &rtyp)),
            );
        }
    }
    REG.with(|r| r.borrow_mut().impls[u].ok = complete);
}

/// Does `t` contain an interface value BY VALUE?
fn contains_dyn(tcx: &TypeCtx, t: &Type, depth: u32) -> bool {
    if depth > 32 {
        return false;
    }
    match t {
        Type::Struct(i) => match tcx.structs.get(*i) {
            Some(s) if s.name.starts_with(P_DYN) => true,
            Some(s) => s
                .fields
                .iter()
                .any(|f| contains_dyn(tcx, &f.ty, depth + 1)),
            None => false,
        },
        Type::Array(e, _) => contains_dyn(tcx, e, depth + 1),
        _ => false,
    }
}

/// `dyn I` in the heap would be a hole in the collector guarantee (see head).
fn check_gc_fields(ck: &mut Checker) {
    if !has_interfaces() {
        return;
    }
    let mut findings: Vec<(String, String)> = Vec::new();
    for s in ck.tcx.structs.iter() {
        let class = match s.name.strip_prefix("gc ") {
            Some(k) => k,
            None => continue,
        };
        for f in &s.fields {
            if contains_dyn(&ck.tcx, &f.ty, 0) {
                findings.push((class.to_string(), f.name.clone()));
            }
        }
    }
    for (class, field) in findings {
        ck.dg.error_note(
            Span::none(),
            format!(
                "field '{}' of the gc class '{}' contains an interface value",
                field, class
            ),
            "the collector traces the heap precisely and does not know the pointer behind 'dyn' (SPEC 3.5.3)".to_string(),
        );
    }
}

/// `// HOOK iface` in `sema::expr_inner`, branch `Cast` — `x as dyn I`.
///
/// The interface value comes about EXPLICITLY (SPEC §6.2: "dynamic
/// resolution through `dyn Interface`, written down explicitly"). There is
/// no silent conversion at an assignment or an argument — Firn has no
/// implicit conversions (SPEC §4.5), and for one that appends a method
/// table this would be the worst place to start.
pub(crate) fn hook_cast(ck: &mut Checker, span: Span, src: &Type, dst: &Type) -> Option<Type> {
    let sidx = match dst {
        Type::Struct(i) => *i,
        _ => return None,
    };
    let iface = ck
        .tcx
        .structs
        .get(sidx)
        .and_then(|s| interface_of(&s.name))?
        .to_string();
    let source = match src {
        Type::Ptr { inner, .. } => match &**inner {
            Type::Struct(j) => *j,
            _ => {
                ck.dg.error_note(
                    span,
                    format!(
                        "an interface value is made from a pointer to a struct, found {}",
                        ck.tcx.name_of(src)
                    ),
                    format!("write '(&x) as dyn {}'", iface),
                );
                return Some(Type::Error);
            }
        },
        _ => {
            ck.dg.error_note(
                span,
                format!(
                    "an interface value is made from a pointer, found {}",
                    ck.tcx.name_of(src)
                ),
                format!("write '(&x) as dyn {}'", iface),
            );
            return Some(Type::Error);
        }
    };
    if impl_ok(&iface, source) {
        return Some(dst.clone());
    }
    let name = ck.tcx.name_of(&Type::Struct(source));
    let known = REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .filter(|x| x.iface == iface && x.struct_idx != usize::MAX)
            .map(|x| x.ty.clone())
            .collect::<Vec<_>>()
    });
    let note = if known.is_empty() {
        format!("no type implements '{}'", iface)
    } else {
        format!("'{}' implement: {}", iface, known.join(", "))
    };
    ck.dg.error_note(
        span,
        format!("'{}' does not implement the interface '{}'", name, iface),
        note,
    );
    Some(Type::Error)
}

/// Does the struct `sidx` implement the interface `iface` completely?
fn impl_ok(iface: &str, sidx: usize) -> bool {
    REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .any(|x| x.iface == iface && x.struct_idx == sidx && x.ok)
    })
}

/// Number of the method in the interface (= place in the method table).
pub(crate) fn slot_of(iface: &str, method: &str) -> Option<usize> {
    let i = iface_index(iface)?;
    REG.with(|r| {
        r.borrow().ifaces[i]
            .methods
            .iter()
            .position(|m| m.name == method)
    })
}

/// Return type of an interface method — for `sema::probe` as well, so that
/// a literal beside it gets its type (`f.area() != 42`).
pub(crate) fn ret_of(iface: &str, method: &str) -> Option<Type> {
    let i = iface_index(iface)?;
    REG.with(|r| {
        r.borrow().ifaces[i]
            .methods
            .iter()
            .find(|m| m.name == method)
            .map(|m| m.rtyp.clone())
    })
}

/// `// HOOK iface` in `impls::hook_call` — `f.m(args)` on a `dyn I`.
///
/// The receiver is a VALUE (the fat pointer itself). A `*dyn I` is
/// deliberately not accepted: `(*z).m(…)` says the same and makes it
/// visible that two words are read.
pub(crate) fn hook_method(
    ck: &mut Checker,
    iface: &str,
    method: &str,
    args: &[Expr],
    et: &Type,
    is_ptr: bool,
    nspan: Span,
    espan: Span,
) -> Type {
    let display = format!("dyn {}.{}", iface, method);
    let ii = match iface_index(iface) {
        Some(i) => i,
        None => return Type::Error,
    };
    let m = match REG.with(|r| {
        r.borrow().ifaces[ii]
            .methods
            .iter()
            .find(|m| m.name == method)
            .cloned()
    }) {
        Some(m) => m,
        None => {
            for a in &args[1..] {
                ck.type_out_expr(a);
            }
            let present: Vec<String> = REG.with(|r| {
                r.borrow().ifaces[ii]
                    .methods
                    .iter()
                    .map(|m| m.name.clone())
                    .collect()
            });
            let note = if present.is_empty() {
                format!("the interface '{}' has no methods", iface)
            } else {
                format!("'{}' has: {}", iface, present.join(", "))
            };
            ck.dg.error_note(
                nspan,
                format!("the interface '{}' has no method '{}'", iface, method),
                note,
            );
            return Type::Error;
        }
    };
    // OBJECT SAFETY (round 50): a signature containing `Self` is not known to
    // the caller through `dyn I` — which type sits behind it is settled only
    // at run time, and `*Self` would be a different type for each. Such
    // methods exist STATICALLY only, through a bound.
    if m.has_self {
        for a in &args[1..] {
            ck.type_out_expr(a);
        }
        ck.dg.error_note(
            nspan,
            format!(
                "'{}.{}' mentions 'Self' and is therefore not callable via 'dyn {}'",
                iface, method, iface
            ),
            format!(
                "call it via a bound: 'fn f[T: {}](x: *T)' — there the type is fixed",
                iface
            ),
        );
        return Type::Error;
    }
    if is_ptr {
        if let Some(recv) = args.first() {
            ck.dg.error_note(
                recv.span,
                format!(
                    "'{}' expects the interface value itself, found {}",
                    display,
                    ck.tcx.name_of(et)
                ),
                format!("write (*x).{}(…)", method),
            );
        }
    }
    let expected = m.ptypes.len();
    let found = args.len().saturating_sub(1);
    if found != expected {
        ck.dg.error(
            espan,
            format!(
                "method '{}' expects {} argument(s), found {}",
                display, expected, found
            ),
        );
    }
    for (i, a) in args[1..].iter().enumerate() {
        match m.ptypes.get(i) {
            Some(p) => ck.check_argument(&display, i + 1, a, p),
            None => ck.type_out_expr(a),
        }
    }
    m.rtyp
}

// ------------------------------------------------------------------ Lowering

/// Method table of an implementation — the key for `Op::VtabAddr`.
/// `None` when this type does not implement the interface (completely).
pub(crate) fn table_of(iface: &str, sidx: usize) -> Option<String> {
    REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .find(|x| x.iface == iface && x.struct_idx == sidx && x.ok)
            .map(|x| table_key(iface, &x.prefix))
    })
}

// ------------------------------------------------------------ Code generator

/// All method tables as one `.rodata` block.
///
/// One table per complete implementation — even for one that never appears
/// in any `as dyn`. That is deliberate: the content hangs off the
/// declaration alone, not off the call sites; a table that comes about only
/// sometimes would be the sort of state you do not want to see while
/// troubleshooting.
pub(crate) fn tables_asm() -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let tables: Vec<(String, Vec<String>)> = REG.with(|r| {
        let reg = r.borrow();
        reg.impls
            .iter()
            .filter(|u| u.ok && u.struct_idx != usize::MAX)
            .filter_map(|u| {
                let ii = reg.ifaces.iter().position(|s| s.name == u.iface)?;
                let targets: Vec<String> = reg.ifaces[ii]
                    .methods
                    .iter()
                    .map(|m| format!("{}{}{}", u.prefix, crate::impls::SEP, m.name))
                    .collect();
                Some((table_key(&u.iface, &u.prefix), targets))
            })
            .collect()
    });
    if tables.is_empty() {
        return out;
    }
    let _ = writeln!(out, "{}", crate::target::reloc_rodata());
    let _ = writeln!(out, "{}", crate::target::align(8));
    for (key, targets) in tables {
        let _ = writeln!(out, "{}{}:", TABLE_LABEL, key);
        for z in targets {
            let _ = writeln!(out, "    .quad {}", crate::codegen_x86::label(&z));
        }
    }
    out
}

/// Signature of an interface method for the lowering: the receiver counts
/// as the first parameter (a pointer), after it the parameters from the
/// declaration. That way the call looks to `lower_call` just like every
/// other one.
fn methods_sig(iface: &str, method: &str) -> Option<crate::sema::FnSig> {
    let i = iface_index(iface)?;
    REG.with(|r| {
        let reg = r.borrow();
        let m = reg.ifaces[i].methods.iter().find(|m| m.name == method)?;
        let mut params = vec![Type::ptr(Type::U8, true)];
        params.extend(m.ptypes.iter().cloned());
        Some(crate::sema::FnSig { params, ret: m.rtyp.clone() })
    })
}

/// `// HOOK iface` in `lower::lower_call` — prepare the dynamic
/// dispatch: read the data pointer, read the method table, read the entry.
///
/// ```text
/// %b = <address of the interface value>
/// %d = load.ptr [%b + 0]      ; the value itself
/// %t = load.ptr [%b + 8]      ; the method table
/// %z = load.ptr [%t + 8*k]    ; the k-th method of the interface
/// ```
///
/// Three loads per call — that is the price of dynamic dispatch, and it
/// stands here visibly (SPEC §1, guiding principle 1: nothing hidden).
pub(crate) fn lower_dispatch(
    lo: &mut crate::lower::Lower,
    iface: &str,
    method: &str,
    recv: &Expr,
    span: Span,
) -> Option<(crate::fir::Val, crate::fir::Val, crate::sema::FnSig)> {
    use crate::fir::{FTy, Op};
    let slot = match slot_of(iface, method) {
        Some(s) => s,
        None => return lo.ice(span, "unknown interface method in lowering"),
    };
    let sig = match methods_sig(iface, method) {
        Some(s) => s,
        None => return lo.ice(span, "interface method without signature in lowering"),
    };
    let base = lo.lower_addr(recv)?;
    let dadr = lo.field_addr_at(base, OFF_DATA);
    let data = lo.load(FTy::Ptr, dadr);
    let tadr = lo.field_addr_at(base, OFF_TABLE);
    let table = lo.load(FTy::Ptr, tadr);
    let eadr = lo.field_addr_at(table, 8 * slot as u64);
    let target = lo.load(FTy::Ptr, eadr);
    Some((target, data, sig))
}

/// `// HOOK iface` in `lower::write_into_inner` — `p as dyn I`.
///
/// Two words: the pointer as it is, and the address of the method table.
/// The data pointer is NOT changed (no offset, no veiling) — only that way
/// does the conservative stack scan of the collector find the object
/// behind it (SPEC §3.5.3, see the head of this file).
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
        _ => return lo.ice(span, "interface value without struct type"),
    };
    let iface = match lo.info.tcx.structs.get(sidx).and_then(|s| interface_of(&s.name)) {
        Some(i) => i.to_string(),
        None => return lo.ice(span, "conversion into a non-interface type"),
    };
    let source = match lo.ty_of(inner) {
        Type::Ptr { inner, .. } => match *inner {
            Type::Struct(j) => j,
            _ => return lo.ice(span, "interface value from a pointer without struct"),
        },
        _ => return lo.ice(span, "interface value from a non-pointer"),
    };
    let key = match table_of(&iface, source) {
        Some(k) => k,
        None => return lo.ice(span, "implementation without method table in lowering"),
    };
    let pv = lo.lower_expr(inner)?;
    let dadr = lo.field_addr_at(addr, OFF_DATA);
    lo.store(FTy::Ptr, dadr, pv);
    let tv = lo.push(FTy::Ptr, Op::VtabAddr { table: key });
    let tadr = lo.field_addr_at(addr, OFF_TABLE);
    lo.store(FTy::Ptr, tadr, tv);
    Some(())
}

/// Assembler name of a method table.
pub(crate) fn table_label(key: &str) -> String {
    format!("{}{}", TABLE_LABEL, key)
}
