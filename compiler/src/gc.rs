// SPDX-License-Identifier: GPL-2.0-only
//! Optional tracing GC — SPEC §3.5 (`S2`–`S6`), inheritance §4.4.
//!
//! This file belongs to the module `gckern` (see PLAN.md, round "hardening
//! test 2"). It holds
//!  * the parser extensions for `gc class C [extends B] { … }`
//!    (wired up through `// HOOK gc` lines in `parser.rs`),
//!  * the registration of the classes, of their field layout, of their type
//!    tag and of their ancestor chain,
//!  * the type check of `Gc[T]`, `GcWeak[T]`, `gc C{…}`, `weak(x)`,
//!    `strong(w)`, `x.as?[T]` and of the free upcast,
//!  * the type table for the collector (`.rodata`, `ty_table_asm`).
//!
//! The lowering to FIR is in `gc_lower.rs`, the collector runtime as
//! readable Firn in `lib/gc/gc.fi`. That runtime is embedded through
//! `include_str!` and pulled in automatically as an extra module as soon as
//! a `gc class` appears in the program (`runtime_source`, `modules.rs`).
//!
//! ## Representation (binding, `docs/GC.md`)
//!
//! ```text
//! gc class C { … }   -> struct "gc C" in types::TypeCtx (prefix layout of the base)
//! Gc[C]              -> *mut <struct "gc C">          (first class pointer)
//! GcWeak[C]          -> struct "GcWeak[C]" { __p: u64, __s: u64 }
//! gc C{ … }          -> AllocError!Gc[C]
//! ```
//!
//! `GcWeak[C]` carries the pointer **veiled** (`__p = p ^ WEAK_XOR`), so that
//! the conservative stack scan does not read it as a strong root, plus the
//! serial number `__s` of the target block, so that a reused block does not
//! pass as the old target.
//!
//! ## Contract to the outside (stable, other modules depend on it)
//!
//! `nogc.rs` (module `nogc`, `#[no_gc]` check per SPEC §3.5.4) uses the two
//! queries `is_gc_alloc_call` and `is_gc_ref` exclusively. Their signatures
//! are fixed.

use std::cell::RefCell;

use crate::ast::{Expr, ExprKind, TypeExpr};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::Type;

// ----------------------------------------------------------------- Namespace

/// Prefix of all compiler internal GC names. It contains `#`, so it can never
/// arise from an identifier of the source text.
/// `gc C{…}` appears as the call `"gc C"` in the AST. The name contains a
/// space, so it can never be an identifier of the source text — and in the
/// `#[no_gc]` message it reads exactly as it stands in the source.
const P_NEW: &str = "gc ";
const P_TY: &str = "__gc#p:";
const P_WTYP: &str = "__gc#w:";
/// The same prefixes for `mono.rs` (round 53: `Gc[T]` in a template).
pub(crate) const P_TY_PUB: &str = P_TY;
pub(crate) const P_WTYP_PUB: &str = P_WTYP;
const P_AS: &str = "__gc#as:";

/// Call names of the runtime (`lib/gc/gc.fi`) that can trigger a collection
/// run or touch the state of the collector.
const RUNTIME_COLLECTS: [&str; 4] = ["gc_init", "gc_collect", "__gc_alloc_raw", "__gc_collect_now"];
/// Further runtime names: pure queries, but part of the collector.
const RUNTIME_QUERY: [&str; 11] = [
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

/// Compiler intrinsics that `gc_lower.rs` turns into `Op::GcAddr`.
pub(crate) const INTR_STATE: &str = "__gc_state";
pub(crate) const INTR_REGS: &str = "__gc_save_regs";

/// Runtime function behind `weak(g)`.
pub(crate) const FN_WEAK: &str = "__gc_weak_raw";
/// Runtime function behind `strong(w)`.
pub(crate) const FN_STRONG: &str = "__gc_strong_raw";
/// Runtime function behind `x.as?[T]`.
pub(crate) const FN_AS: &str = "__gc_as_raw";
/// Runtime function behind `gc C{…}`.
pub(crate) const FN_ALLOC: &str = "__gc_alloc_raw";
/// Insertion barrier when writing a Gc pointer into the heap.
pub(crate) const FN_BARRIER: &str = "__gc_barrier";
/// Error set of the fallible allocation (DESIGN_GOALS §2).
pub(crate) const ERR_SET: &str = "AllocError";
/// **Round 47** — dispatcher of the finalizers (`SPEC` §3.5.3 `S4`).
///
/// While collecting, the runtime calls `__gc_finalize(kind, p)`. Stage 0 has
/// no function pointers; a dispatcher function with a tag is the honest
/// equivalent and needs no indirect call in the code generator (R46 builds
/// that one for vtables, not this round).
///
/// If the **root file** of the program declares this function itself, the
/// compiler takes it; otherwise it adds the empty default. Only the root
/// file counts, because inside a module the name turns into `module__…`
/// and the runtime would no longer find it.
pub(crate) const FN_FINAL: &str = "__gc_finalize";

/// **Round 49** — dispatcher of the thread work (`lib/gc/gc.fi`,
/// `thread_start`).
/// The same way as with the dispatcher of the finalizers and for the same
/// reason: stage 0 has no function pointers, so a thread carries a KIND OF
/// WORK rather than an address. If the root file declares the function
/// itself, the compiler takes it; otherwise it adds the empty default.
pub(crate) const FN_THREAD: &str = "__thread_work";

// ----------------------------------------------------------------- Data model

#[derive(Clone, Debug)]
struct Field {
    name: String,
    ty: TypeExpr,
    span: Span,
}

#[derive(Clone, Debug)]
struct Class {
    name: String,
    span: Span,
    base: Option<(String, Span)>,
    fields: Vec<Field>,
    /// index into `types::TypeCtx` (name `"gc C"`), `usize::MAX` until registered
    struct_idx: usize,
    /// index of the struct `GcWeak[C]`
    weak_idx: usize,
    /// type tag, from 1 upwards by declaration order
    tid: u64,
    /// after the registration: size in bytes
    size: u64,
    /// offsets of the `Gc[T]` fields (precise heap tracing)
    strong_offs: Vec<u64>,
    /// offsets of the `GcWeak[T]` fields (for statistics/documentation only)
    weak_offs: Vec<u64>,
    /// type tag of the base, 0 = none
    base_tid: u64,
    /// struct index of the error union `AllocError!Gc[C]` (`usize::MAX` = not yet)
    union_idx: usize,
    /// the layout is registered
    done: bool,
}

#[derive(Default)]
struct Registry {
    classes: Vec<Class>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

fn index_of(name: &str) -> Option<usize> {
    REG.with(|r| r.borrow().classes.iter().position(|k| k.name == name))
}

/// Is this name a declared `gc class`?
pub(crate) fn is_class(name: &str) -> bool {
    index_of(name).is_some()
}

/// Is there any `gc class` at all in this compilation?
pub(crate) fn has_classes() -> bool {
    REG.with(|r| !r.borrow().classes.is_empty())
}

/// Resets the registry (one per compilation, `parser::reset_hooks`).
pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

/// Round 49: the runtime is part of this program. Set in `modules.rs`,
/// where it really enters the file list — not in `runtime_source`, whose
/// result is built in tests as well.
pub(crate) fn runtime_remember() {
    RUNTIME_INSIDE.with(|c| c.set(true));
}

/// Reset before every compilation (one process can compile several —
/// `cargo test`).
pub(crate) fn runtime_reset() {
    RUNTIME_INSIDE.with(|c| c.set(false));
}

// ------------------------------------------------------- Contract for nogc.rs

/// Is this the call name of a GC allocation or of a collector function that
/// can trigger a collection run (`gc C{…}`, `gc_collect`, …)?
///
/// Exactly those calls are forbidden inside a `#[no_gc]` function.
pub(crate) fn is_gc_alloc_call(name: &str) -> bool {
    // ROUND 70: `a + b` on `str` allocates in the GC heap and is therefore
    // forbidden inside a `#[no_gc]` call tree, exactly like `gc C{…}`.
    if name == crate::strtype::FN_CONCAT {
        return true;
    }
    if name.starts_with(P_NEW) {
        return true;
    }
    // The module path in front of it counts as well (`module__gc_collect`),
    // otherwise the guarantee could be dodged across a module boundary.
    let blank = name.rsplit("__").next().unwrap_or(name);
    RUNTIME_COLLECTS.contains(&name)
        || RUNTIME_COLLECTS.contains(&blank)
        || RUNTIME_QUERY.contains(&name)
        || RUNTIME_QUERY.contains(&blank)
        || name == INTR_STATE
        || name == INTR_REGS
        || name == FN_WEAK
        || name == FN_STRONG
        || name == FN_AS
        || name == FN_BARRIER
}

/// Is `t` a GC pointer type (`Gc[T]` or `GcWeak[T]`)? Writing into a field
/// of this type needs the insertion barrier and is forbidden inside
/// `#[no_gc]`.
pub(crate) fn is_gc_ref(t: &Type) -> bool {
    is_gc_ptr(t) || is_gc_weak(t)
}

/// `Gc[T]` — strong, first class pointer.
pub(crate) fn is_gc_ptr(t: &Type) -> bool {
    match t {
        Type::Ptr { inner, .. } => match **inner {
            Type::Struct(i) => class_of_struct(i).is_some(),
            _ => false,
        },
        _ => false,
    }
}

/// `GcWeak[T]` — weak reference (two word struct).
pub(crate) fn is_gc_weak(t: &Type) -> bool {
    match t {
        Type::Struct(i) => REG.with(|r| r.borrow().classes.iter().any(|k| k.weak_idx == *i)),
        _ => false,
    }
}

fn class_of_struct(idx: usize) -> Option<usize> {
    REG.with(|r| r.borrow().classes.iter().position(|k| k.struct_idx == idx))
}

// ------------------------------------------------------------- Parser hooks

impl<'a> Parser<'a> {
    /// `gc class C [extends B] { field: Type, … }`
    fn gc_class_decl(&mut self) {
        let start = self.bump(); // 'gc'
        self.bump(); // 'class'
        let (name, nspan) = match self.ident("after 'gc class'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        let mut base: Option<(String, Span)> = None;
        if matches!(self.kind(), TokKind::Ident(n) if n == "extends") {
            self.bump();
            match self.ident("after 'extends'") {
                Some((b, example)) => base = Some((b, example)),
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
                    "multiple inheritance is not allowed".to_string(),
                    "SPEC 4.4: 'gc class' has at most ONE base",
                );
                self.recovering = false;
                self.sync_item();
                return;
            }
        }
        if !self.expect(TokKind::LBrace, "after the name of the gc class") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let mut fields: Vec<Field> = Vec::new();
        loop {
            while self.eat(&TokKind::Comma) {}
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (fname, fspan) = match self.ident("for a field of the gc class") {
                Some(x) => x,
                None => break,
            };
            if !self.expect(TokKind::Colon, "after the field name") {
                break;
            }
            let ty = match self.parse_type() {
                Some(t) => t,
                None => break,
            };
            if fields.iter().any(|f| f.name == fname) {
                self.dg.error(
                    fspan,
                    format!("field '{}' is already declared in 'gc class {}'", fname, name),
                );
            } else {
                fields.push(Field { name: fname, ty, span: fspan });
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
        self.close(TokKind::RBrace, "at the end of the gc class");
        self.recovering = false;
        let span = Parser::join(start, end);
        let _ = span;
        if index_of(&name).is_some() {
            self.dg
                .error(nspan, format!("'gc class {}' is already declared", name));
            return;
        }
        REG.with(|r| {
            let mut reg = r.borrow_mut();
            let tid = reg.classes.len() as u64 + 1;
            reg.classes.push(Class {
                name: name.clone(),
                span: nspan,
                base,
                fields,
                struct_idx: usize::MAX,
                weak_idx: usize::MAX,
                tid,
                size: 0,
                strong_offs: Vec::new(),
                weak_offs: Vec::new(),
                base_tid: 0,
                union_idx: usize::MAX,
                done: false,
            });
        });
    }

    /// Round 53: `[E]` or `[K, V]` after `GcVec`/`GcMap`. The arguments are
    /// FULL types (`GcVec[Gc[Node]]`), are checked and then discarded — the
    /// container is nominally one. Yields the span of the closing
    /// bracket.
    fn gc_collection_args(&mut self, name: &str, n: usize) -> Option<Span> {
        if !self.expect(TokKind::LBracket, "after 'GcVec'/'GcMap'") {
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
                format!("'{}' expects {} type argument(s), got {}", name, n, i),
                "GcVec[E] has one, GcMap[K, V] has two (SPEC 3.5.2)",
            );
            self.recovering = true;
            return None;
        }
        let end = self.span();
        if !self.expect(TokKind::RBracket, "after the type arguments") {
            return None;
        }
        Some(end)
    }

    /// `[C]` after `Gc`/`GcWeak`/`gc_null`/`weak_null`/`as?`.
    fn gc_ty_arg(&mut self, what: &str) -> Option<(String, Span)> {
        if !self.expect(TokKind::LBracket, what) {
            return None;
        }
        let r = self.ident(what)?;
        if !self.expect(TokKind::RBracket, "after the type argument") {
            return None;
        }
        Some(r)
    }
}

/// `// HOOK gc` in `parser.rs::program` — `gc class` declaration.
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    let is_gc = matches!(p.kind(), TokKind::Ident(n) if n == "gc");
    if !is_gc {
        return false;
    }
    let is_class = matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::Ident(n)) if n == "class");
    if !is_class {
        return false;
    }
    p.gc_class_decl();
    true
}

/// `// HOOK gc` in `parser.rs::parse_type_inner` — `Gc[C]`, `GcWeak[C]`.
/// The name is consumed already, `sp` is its position.
pub(crate) fn hook_type(p: &mut Parser, name: &str, sp: Span) -> Option<TypeExpr> {
    // Round 53: `GcVec[E]` and `GcMap[K,V]` (SPEC §3.5.2). Both are
    // POINTERS to the runtime classes `GcVec`/`GcMap` from lib/gc/gcvec.fi
    // and lib/gc/gcmap.fi — `GcVec[Gc[Node]]` is therefore exactly
    // `Gc[GcVec]`, merely written the way the SPEC states it.
    //
    // WHAT THIS IS NOT, and that belongs here: a SEPARATE class per element
    // type. Stage 0 has no generic `gc class` — the container is nominally
    // ONE, and the element type is checked at the ACCESS
    // (`gcvec_append[Node](…)`), not at the field. The type arguments are
    // parsed here completely (`Gc[Node]` too), so that a typo shows up, but
    // discarded afterwards. `docs/ROUND53.md` §6 names the price.
    if name == "GcVec" || name == "GcMap" {
        if !p.at(&TokKind::LBracket) {
            return None;
        }
        let n = if name == "GcVec" { 1 } else { 2 };
        let ksp = p.gc_collection_args(name, n)?;
        return Some(TypeExpr::Named(
            format!("{}{}", P_TY, name),
            Parser::join(sp, ksp),
        ));
    }
    let prefix = match name {
        "Gc" => P_TY,
        "GcWeak" => P_WTYP,
        _ => return None,
    };
    if !p.at(&TokKind::LBracket) {
        return None;
    }
    let (class, ksp) = p.gc_ty_arg("after 'Gc'/'GcWeak'")?;
    Some(TypeExpr::Named(format!("{}{}", prefix, class), Parser::join(sp, ksp)))
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
            // `gc C{ … }` — allocation on the GC heap.
            let class = match p.toks.get(p.pos + 1).map(|t| t.kind.clone()) {
                Some(TokKind::Ident(k)) if k != "class" => k,
                _ => return None,
            };
            if !matches!(p.toks.get(p.pos + 2).map(|t| &t.kind), Some(TokKind::LBrace)) {
                return None;
            }
            let sp = p.bump(); // 'gc'
            let ksp = p.bump(); // class label
            let span = Parser::join(sp, ksp);
            let saved = p.no_struct_lit;
            p.no_struct_lit = false;
            let lit = p.struct_lit(format!("{}{}", P_NEW, class), span);
            p.no_struct_lit = saved;
            // Wrapped as a CALL: only that way does the `#[no_gc]` check
            // (nogc.rs, rule 1) see the allocation — it checks call names.
            let full = Parser::join(span, lit.span);
            Some(p.mk(full, ExprKind::Call(format!("{}{}", P_NEW, class), vec![lit], span)))
        }
        "gc_null" | "weak_null" => {
            if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::LBracket)) {
                return None;
            }
            let sp = p.bump();
            let (class, ksp) = p.gc_ty_arg("after 'gc_null'/'weak_null'")?;
            let span = Parser::join(sp, ksp);
            if !p.expect(TokKind::LParen, "after the type argument") {
                return None;
            }
            if !p.expect(TokKind::RParen, "after '(' — the null value has no argument") {
                return None;
            }
            if name == "gc_null" {
                // `0 as Gc[C]` — the null value is the null pointer.
                let null = p.mk(span, ExprKind::Int(0));
                Some(p.mk(
                    span,
                    ExprKind::Cast(
                        Box::new(null),
                        TypeExpr::Named(format!("{}{}", P_TY, class), span),
                    ),
                ))
            } else {
                // `GcWeak[C]{ __p: 0, __s: 0 }` — the empty weak reference.
                let null1 = p.mk(span, ExprKind::Int(0));
                let null2 = p.mk(span, ExprKind::Int(0));
                Some(p.mk(
                    span,
                    ExprKind::StructLit(
                        weak_struct_name(&class),
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
/// Sits right after the consumed '.'.
pub(crate) fn hook_postfix(p: &mut Parser, base: &Expr) -> Option<Expr> {
    if !matches!(p.kind(), TokKind::KwAs) {
        return None;
    }
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::Question)) {
        return None;
    }
    let sp = p.bump(); // 'as'
    p.bump(); // '?'
    let (class, ksp) = p.gc_ty_arg("after '.as?'")?;
    let span = Parser::join(base.span, ksp);
    Some(p.mk(
        span,
        ExprKind::Call(format!("{}{}", P_AS, class), vec![base.clone()], Parser::join(sp, ksp)),
    ))
}

fn weak_struct_name(class: &str) -> String {
    format!("GcWeak[{}]", class)
}

// ------------------------------------------- Registration at the type context

/// `// HOOK gc` in `sema::Checker::run` (before `collect_structs`):
/// registers every `gc class` as a struct with prefix layout and computes
/// the type tag, the ancestor chain and the offsets for precise heap tracing.
pub(crate) fn declare_classes(ck: &mut Checker) {
    let n = REG.with(|r| r.borrow().classes.len());
    if n == 0 {
        return;
    }
    // 1. Register the structs (the layout arrives at step 3).
    for i in 0..n {
        let (name, span) =
            match REG.with(|r| r.borrow().classes.get(i).map(|k| (k.name.clone(), k.span))) {
                Some(x) => x,
                None => continue,
            };
        if ck.tcx.lookup(&name).is_some() {
            ck.dg
                .error(span, format!("type '{}' is already declared", name));
        }
        let sidx = ck.tcx.declare(&format!("gc {}", name));
        let widx = ck.tcx.declare(&weak_struct_name(&name));
        ck.tcx
            .set_fields(widx, vec![("__p".to_string(), Type::U64), ("__s".to_string(), Type::U64)]);
        REG.with(|r| {
            let mut reg = r.borrow_mut();
            if let Some(k) = reg.classes.get_mut(i) {
                k.struct_idx = sidx;
                k.weak_idx = widx;
            }
        });
    }
    // 2. Check the base (existence, no circles).
    for i in 0..n {
        let (name, base) =
            match REG.with(|r| r.borrow().classes.get(i).map(|k| (k.name.clone(), k.base.clone())))
            {
                Some(x) => x,
                None => continue,
            };
        let (bname, bspan) = match base {
            Some(b) => b,
            None => continue,
        };
        let bi = match index_of(&bname) {
            Some(b) => b,
            None => {
                ck.dg.error_note(
                    bspan,
                    format!("unknown base class '{}'", bname),
                    "a base must itself be declared with 'gc class' (SPEC 4.4)",
                );
                REG.with(|r| {
                    if let Some(k) = r.borrow_mut().classes.get_mut(i) {
                        k.base = None;
                    }
                });
                continue;
            }
        };
        if circle(bi, i) {
            ck.dg.error(
                bspan,
                format!("the inheritance chain of 'gc class {}' is cyclic", name),
            );
            REG.with(|r| {
                if let Some(k) = r.borrow_mut().classes.get_mut(i) {
                    k.base = None;
                }
            });
        }
    }
}

/// `// HOOK gc` in `sema::add_items_inner` (AFTER `collect_structs`):
/// settles the field layout of every class. Only here are the structs of the
/// program known — that way a struct field in a gc class gets the right
/// message instead of "unknown type".
pub(crate) fn layout_classes(ck: &mut Checker) {
    let n = REG.with(|r| r.borrow().classes.len());
    // Layout by topological order (base first).
    for _ in 0..n {
        let mut progress = false;
        for i in 0..n {
            let (done, base) = REG.with(|r| {
                let reg = r.borrow();
                match reg.classes.get(i) {
                    Some(k) => (k.done, k.base.clone()),
                    None => (true, None),
                }
            });
            if done {
                continue;
            }
            if let Some((b, _)) = &base {
                let bfertig = index_of(b)
                    .and_then(|bi| REG.with(|r| r.borrow().classes.get(bi).map(|k| k.done)))
                    .unwrap_or(true);
                if !bfertig {
                    continue;
                }
            }
            put_out(ck, i);
            progress = true;
        }
        if !progress {
            break;
        }
    }
}

/// Does `from` reach `target` through the base chain?
fn circle(of: usize, target: usize) -> bool {
    let mut cur = of;
    for _ in 0..1024 {
        if cur == target {
            return true;
        }
        let b = REG.with(|r| r.borrow().classes.get(cur).and_then(|k| k.base.clone()));
        match b.and_then(|(n, _)| index_of(&n)) {
            Some(next) => cur = next,
            None => return false,
        }
    }
    true
}

fn put_out(ck: &mut Checker, i: usize) {
    let k = match REG.with(|r| r.borrow().classes.get(i).cloned()) {
        Some(k) => k,
        None => return,
    };
    // Base fields sit AT THE FRONT (prefix layout, free upcast).
    let mut fields: Vec<(String, Type)> = Vec::new();
    let mut base_tid = 0u64;
    if let Some((bname, _)) = &k.base {
        if let Some(bi) = index_of(bname) {
            let (bidx, btid) = REG.with(|r| {
                let reg = r.borrow();
                match reg.classes.get(bi) {
                    Some(b) => (b.struct_idx, b.tid),
                    None => (usize::MAX, 0),
                }
            });
            base_tid = btid;
            if let Some(bd) = ck.tcx.structs.get(bidx) {
                for f in &bd.fields {
                    fields.push((f.name.clone(), f.ty.clone()));
                }
            }
        }
    }
    for f in &k.fields {
        if fields.iter().any(|(n, _)| *n == f.name) {
            ck.dg.error_note(
                f.span,
                format!("field '{}' is already taken in the base of 'gc class {}'", f.name, k.name),
                "inherited field names must not be assigned again (SPEC 4.4)",
            );
            continue;
        }
        let t = ck.resolve_ty(&f.ty);
        if !field_ty_allowed(&t) {
            ck.dg.error_note(
                f.span,
                format!(
                    "field type {} is not allowed in a gc class",
                    ck.tcx.name_of(&t)
                ),
                "allowed are integers, bool, pointers, Gc[T], GcWeak[T] and arrays of these",
            );
            continue;
        }
        fields.push((f.name.clone(), t));
    }
    let sidx = k.struct_idx;
    if sidx == usize::MAX {
        return;
    }
    ck.tcx.set_fields(sidx, fields);
    // Collect the offsets for the compiler generated tracing.
    let mut strong = Vec::new();
    let mut weak = Vec::new();
    let mut size = 0;
    if let Some(d) = ck.tcx.structs.get(sidx) {
        size = d.size;
        for f in &d.fields {
            if is_gc_ptr(&f.ty) {
                strong.push(f.offset);
            } else if is_gc_weak(&f.ty) {
                weak.push(f.offset);
            }
        }
    }
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        if let Some(k) = reg.classes.get_mut(i) {
            k.size = size;
            k.strong_offs = strong;
            k.weak_offs = weak;
            k.base_tid = base_tid;
            k.done = true;
        }
    });
}

fn field_ty_allowed(t: &Type) -> bool {
    match t {
        Type::Error => true, // the error got reported already
        Type::Array(e, _) => field_ty_allowed(e),
        Type::Struct(_) => is_gc_weak(t),
        Type::Void | Type::UntypedInt => false,
        Type::Ptr { .. } => true,
        _ => true,
    }
}

// --------------------------------------------------------------- Type check

/// `// HOOK gc` in `sema::resolve_ty_d`: `Gc[C]`, `GcWeak[C]` and the
/// forbidden use of a `gc class` name as an ordinary type.
pub(crate) fn hook_resolve_ty(ck: &mut Checker, te: &TypeExpr) -> Option<Type> {
    let (name, span) = match te {
        TypeExpr::Named(n, s) => (n.as_str(), *s),
        _ => return None,
    };
    if let Some(class) = name.strip_prefix(P_TY) {
        return Some(match index_of(class) {
            Some(i) => {
                let sidx = REG.with(|r| r.borrow().classes[i].struct_idx);
                Type::ptr(Type::Struct(sidx), true)
            }
            None => {
                unknown_class(ck, class, span);
                Type::Error
            }
        });
    }
    if let Some(class) = name.strip_prefix(P_WTYP) {
        return Some(match index_of(class) {
            Some(i) => Type::Struct(REG.with(|r| r.borrow().classes[i].weak_idx)),
            None => {
                unknown_class(ck, class, span);
                Type::Error
            }
        });
    }
    // `let x: Node` — a gc class value lives on the GC heap ONLY.
    if is_class(name) {
        ck.dg.error_note(
            span,
            format!("'{}' is a gc class and cannot be a value", name),
            "a 'gc class' value lives only on the GC heap: write 'Gc[".to_string()
                + name
                + "]' (SPEC 3.5.1)",
        );
        return Some(Type::Error);
    }
    None
}

fn unknown_class(ck: &mut Checker, name: &str, span: Span) {
    ck.dg.error_note(
        span,
        format!("unknown gc class '{}'", name),
        "a gc class is declared with 'gc class Name { … }'",
    );
}

/// `gc C{ … }` yields `AllocError!Gc[C]` (called from `hook_call`).
fn check_new(
    ck: &mut Checker,
    name: &str,
    fields: &[(String, Expr, Span)],
    nspan: Span,
) -> Option<Type> {
    let class = name.strip_prefix(P_NEW)?;
    let i = match index_of(class) {
        Some(i) => i,
        None => {
            unknown_class(ck, class, nspan);
            for (_, e, _) in fields {
                ck.type_out_expr(e);
            }
            return Some(Type::Error);
        }
    };
    let sidx = REG.with(|r| r.borrow().classes[i].struct_idx);
    let decl: Vec<(String, Type)> = match ck.tcx.structs.get(sidx) {
        Some(d) => d.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect(),
        None => Vec::new(),
    };
    let mut seen: Vec<&str> = Vec::new();
    for (fname, fexpr, fspan) in fields {
        match decl.iter().find(|(n, _)| n == fname) {
            Some((_, want)) => {
                let got = ck.expr(fexpr, Some(want));
                if !got.is_error() && !assignable(&got, want) {
                    ck.dg.error(
                        *fspan,
                        format!(
                            "field '{}' of 'gc class {}' expects {}, found {}",
                            fname,
                            class,
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
                    format!("'gc class {}' has no field '{}'", class, fname),
                );
            }
        }
        if seen.contains(&fname.as_str()) {
            ck.dg
                .error(*fspan, format!("field '{}' is given twice", fname));
        }
        seen.push(fname);
    }
    let missing: Vec<String> = decl
        .iter()
        .filter(|(n, _)| !seen.contains(&n.as_str()))
        .map(|(n, _)| n.clone())
        .collect();
    if !missing.is_empty() {
        ck.dg.error_note(
            nspan,
            format!(
                "the fields are missing in 'gc {}{{…}}': {}",
                class,
                missing.join(", ")
            ),
            "in a gc allocation ALL fields must be given",
        );
    }
    let u = alloc_union(ck, Type::ptr(Type::Struct(sidx), true), nspan);
    if let Type::Struct(ui) = u {
        REG.with(|r| {
            if let Some(k) = r.borrow_mut().classes.get_mut(i) {
                k.union_idx = ui;
            }
        });
    }
    Some(u)
}

/// `AllocError!T` — the fallible allocation (DESIGN_GOALS §2).
fn alloc_union(ck: &mut Checker, val: Type, span: Span) -> Type {
    match crate::errors::union_type(ck, ERR_SET, &val) {
        Some(t) => t,
        None => {
            ck.dg.error_note(
                span,
                format!("the error set '{}' is not declared", ERR_SET),
                "it comes with the GC runtime (lib/gc/gc.fi) and is pulled in automatically",
            );
            Type::Error
        }
    }
}

/// `// HOOK gc` in `sema::call`: `weak(g)`, `strong(w)`, `x.as?[C]` and
/// the two compiler intrinsics. Yields `None` when it is none of those.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if let Some(lit) = name.strip_prefix(P_NEW) {
        let _ = lit;
        let fields: Vec<(String, Expr, Span)> = match args.first().map(|a| &a.kind) {
            Some(ExprKind::StructLit(_, f, _)) => f.clone(),
            _ => Vec::new(),
        };
        let t = check_new(ck, name, &fields, nspan)?;
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
    if let Some(class) = name.strip_prefix(P_AS) {
        return Some(check_as(ck, class, args, nspan));
    }
    if (name != "weak" && name != "strong") || ck.fns.contains_key(name) {
        return None;
    }
    if args.len() != 1 {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error(
            espan,
            format!("'{}' expects exactly one argument, found {}", name, args.len()),
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
        return Some(match class_of_struct(sidx) {
            Some(i) => Type::Struct(REG.with(|r| r.borrow().classes[i].weak_idx)),
            None => {
                ck.dg.error_note(
                    args[0].span,
                    format!(
                        "'weak' expects a Gc[T], found {}",
                        ck.tcx.name_of(&at)
                    ),
                    "a weak reference is made only from a strong one",
                );
                Type::Error
            }
        });
    }
    // strong(w)
    let idx = match &at {
        Type::Struct(i) => *i,
        _ => usize::MAX,
    };
    let hit = REG.with(|r| r.borrow().classes.iter().position(|k| k.weak_idx == idx));
    Some(match hit {
        Some(i) => {
            let sidx = REG.with(|r| r.borrow().classes[i].struct_idx);
            Type::ptr(Type::Struct(sidx), true)
        }
        None => {
            ck.dg.error_note(
                args[0].span,
                format!(
                    "'strong' expects a GcWeak[T], found {}",
                    ck.tcx.name_of(&at)
                ),
                "'strong' upgrades a weak reference",
            );
            Type::Error
        }
    })
}

fn check_as(ck: &mut Checker, class: &str, args: &[Expr], nspan: Span) -> Type {
    let target = match index_of(class) {
        Some(i) => i,
        None => {
            for a in args {
                ck.type_out_expr(a);
            }
            unknown_class(ck, class, nspan);
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
    let source = match &at {
        Type::Ptr { inner, .. } => match **inner {
            Type::Struct(i) => class_of_struct(i),
            _ => None,
        },
        _ => None,
    };
    let source = match source {
        Some(q) => q,
        None => {
            ck.dg.error(
                arg.span,
                format!(
                    "'.as?[{}]' expects a Gc[T], found {}",
                    class,
                    ck.tcx.name_of(&at)
                ),
            );
            return Type::Error;
        }
    };
    // Checked downcast: the target must have the source in its ancestor chain.
    if source != target && !is_descendant(target, source) && !is_descendant(source, target) {
        let (qn, zn) = REG.with(|r| {
            let reg = r.borrow();
            (reg.classes[source].name.clone(), reg.classes[target].name.clone())
        });
        ck.dg.error_note(
            nspan,
            format!("'{}' and '{}' are not related", qn, zn),
            "'.as?[T]' only checks within an inheritance chain (SPEC 4.4)",
        );
        return Type::Error;
    }
    let sidx = REG.with(|r| r.borrow().classes[target].struct_idx);
    Type::ptr(Type::Struct(sidx), true)
}

/// Is `a` a descendant of `b` (the same class excluded)?
fn is_descendant(a: usize, b: usize) -> bool {
    let mut cur = a;
    for _ in 0..1024 {
        let base = REG.with(|r| r.borrow().classes.get(cur).and_then(|k| k.base.clone()));
        match base.and_then(|(n, _)| index_of(&n)) {
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

fn assignable(got: &Type, want: &Type) -> bool {
    if got == want {
        return true;
    }
    is_upward(got, want)
}

/// Free upcast `Gc[Element]` -> `Gc[Node]` (SPEC §4.4).
pub(crate) fn is_upward(got: &Type, want: &Type) -> bool {
    let (g, w) = match (got, want) {
        (Type::Ptr { inner: a, .. }, Type::Ptr { inner: b, .. }) => (a, b),
        _ => return false,
    };
    let (gi, wi) = match (&**g, &**w) {
        (Type::Struct(a), Type::Struct(b)) => (*a, *b),
        _ => return false,
    };
    match (class_of_struct(gi), class_of_struct(wi)) {
        (Some(a), Some(b)) => a == b || is_descendant(a, b),
        _ => false,
    }
}

/// Are two Gc pointers related (for the identity comparison `g == h`)?
pub(crate) fn is_related(a: &Type, b: &Type) -> bool {
    is_upward(a, b) || is_upward(b, a)
}

/// `// HOOK gc` in `sema::probe_d`: type of `weak(g)`, `strong(w)` and
/// `x.as?[C]` WITHOUT a check — so that an integer literal beside it gets
/// its type (`if g.field != 5`).
pub(crate) fn probe_ty(name: &str, arg: Option<&Type>) -> Option<Type> {
    if let Some(class) = name.strip_prefix(P_AS) {
        let i = index_of(class)?;
        return Some(Type::ptr(Type::Struct(REG.with(|r| r.borrow().classes[i].struct_idx)), true));
    }
    let at = arg?;
    match name {
        "weak" => {
            let i = match at {
                Type::Ptr { inner, .. } => match **inner {
                    Type::Struct(s) => class_of_struct(s)?,
                    _ => return None,
                },
                _ => return None,
            };
            Some(Type::Struct(REG.with(|r| r.borrow().classes[i].weak_idx)))
        }
        "strong" => {
            let idx = match at {
                Type::Struct(i) => *i,
                _ => return None,
            };
            let i = REG.with(|r| r.borrow().classes.iter().position(|k| k.weak_idx == idx))?;
            Some(Type::ptr(Type::Struct(REG.with(|r| r.borrow().classes[i].struct_idx)), true))
        }
        _ => None,
    }
}

/// `// HOOK gc` in `sema::field_type`: field access through `Gc[T]`.
/// Yields the struct index when `base` is a Gc pointer.
pub(crate) fn hook_field_base(base: &Type) -> Option<usize> {
    match base {
        Type::Ptr { inner, .. } => match **inner {
            Type::Struct(i) if class_of_struct(i).is_some() => Some(i),
            _ => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------- Entries for the lowering

/// Type tag, size and struct index of a class (for `gc_lower.rs`).
pub(crate) fn class_info(name: &str) -> Option<(u64, u64, usize)> {
    let i = index_of(name)?;
    REG.with(|r| {
        let reg = r.borrow();
        reg.classes.get(i).map(|k| (k.tid, k.size, k.struct_idx))
    })
}

/// Struct index of the error union `AllocError!Gc[C]` (for `gc_lower.rs`).
pub(crate) fn union_idx(name: &str) -> Option<usize> {
    let i = index_of(name)?;
    let u = REG.with(|r| r.borrow().classes.get(i).map(|k| k.union_idx))?;
    if u == usize::MAX {
        None
    } else {
        Some(u)
    }
}

/// Name of the class behind a `gc C{…}` or `x.as?[C]` call.
pub(crate) fn class_out_new(name: &str) -> Option<&str> {
    name.strip_prefix(P_NEW)
}

pub(crate) fn class_out_as(name: &str) -> Option<&str> {
    name.strip_prefix(P_AS)
}

// ------------------------------------------------------------ Code generator

/// Offset of the register rescue area in the state block (bytes).
pub(crate) const REG_SAVE_OFF: u64 = 3968;
/// Size of the state block (bytes).
pub(crate) const STATE_SIZE: u64 = 4096;
/// Label of the state block (`.data`, file local).
pub(crate) const STATE_LABEL: &str = ".L__gc_state";
/// Label of the type table (`.rodata`, file local).
pub(crate) const TABLE_LABEL: &str = ".L__gc_typetable";

/// The compiler generated type table: one entry of 8 words per type, built
/// from the field layout (SPEC §3.5.3 — precise heap tracing).
///
/// ```text
/// word 0: n_types
/// entry(tid) = table + 8 + (tid-1)*64
///   +0  size as bytes
///   +8  type tag of the base (0 = none)
///   +16 count of strong fields
///   +24 address of the offset list (Gc[T])
///   +32 count of weak fields
///   +40 address of the offset list (GcWeak[T])
///   +48 type tag (for the probe)
///   +56 reserved
/// ```
/// **Round 58** (fnval.rs) — registers the class of a CAPTURE RECORD.
///
/// The ordinary path (`declare_classes`/`layout_classes`) runs over the
/// items and is long finished by the time a closure is checked: only there
/// are the types of the captured values known. So the class is built here
/// completely, with layout, type tag and traced offsets — from then on the
/// collector treats it like any other, through the same type table.
///
/// Word 0 of the record is the code address and is deliberately NOT traced:
/// it points into `.text`, which is not a heap block.
pub(crate) fn declare_capture_class(
    ck: &mut Checker,
    name: &str,
    caps: &[Type],
) -> (u64, u64, usize) {
    let sidx = ck.tcx.declare(&format!("gc {}", name));
    let mut fields: Vec<(String, Type)> = vec![("__code".to_string(), Type::U64)];
    for (i, t) in caps.iter().enumerate() {
        fields.push((format!("__c{}", i), t.clone()));
    }
    ck.tcx.set_fields(sidx, fields);
    let mut strong = Vec::new();
    let mut size = 0;
    if let Some(d) = ck.tcx.structs.get(sidx) {
        size = d.size;
        for f in &d.fields {
            if is_gc_ptr(&f.ty) {
                strong.push(f.offset);
            }
        }
    }
    let tid = REG.with(|r| {
        let mut reg = r.borrow_mut();
        let tid = reg.classes.len() as u64 + 1;
        reg.classes.push(Class {
            name: name.to_string(),
            span: Span::none(),
            base: None,
            fields: Vec::new(),
            struct_idx: sidx,
            weak_idx: usize::MAX,
            tid,
            size,
            strong_offs: strong,
            weak_offs: Vec::new(),
            base_tid: 0,
            union_idx: usize::MAX,
            done: true,
        });
        tid
    });
    (tid, size, sidx)
}

pub(crate) fn ty_table_asm() -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    REG.with(|r| {
        let reg = r.borrow();
        let _ = writeln!(out, ".section .rodata");
        let _ = writeln!(out, "{}", crate::target::align(8));
        let _ = writeln!(out, "{}:", TABLE_LABEL);
        let _ = writeln!(out, "    .quad {}", reg.classes.len());
        for k in &reg.classes {
            let _ = writeln!(out, "    .quad {}", k.size);
            let _ = writeln!(out, "    .quad {}", k.base_tid);
            let _ = writeln!(out, "    .quad {}", k.strong_offs.len());
            let _ = writeln!(out, "    .quad {}.s{}", TABLE_LABEL, k.tid);
            let _ = writeln!(out, "    .quad {}", k.weak_offs.len());
            let _ = writeln!(out, "    .quad {}.w{}", TABLE_LABEL, k.tid);
            let _ = writeln!(out, "    .quad {}", k.tid);
            let _ = writeln!(out, "    .quad 0");
        }
        for k in &reg.classes {
            let _ = writeln!(out, "{}.s{}:", TABLE_LABEL, k.tid);
            for o in &k.strong_offs {
                let _ = writeln!(out, "    .quad {}", o);
            }
            let _ = writeln!(out, "    .quad 0");
            let _ = writeln!(out, "{}.w{}:", TABLE_LABEL, k.tid);
            for o in &k.weak_offs {
                let _ = writeln!(out, "    .quad {}", o);
            }
            let _ = writeln!(out, "    .quad 0");
        }
        // State block: writable, word 0 points to the type table.
        let _ = writeln!(out, ".section .data");
        let _ = writeln!(out, "{}", crate::target::align(16));
        let _ = writeln!(out, "{}:", STATE_LABEL);
        let _ = writeln!(out, "    .quad {}", TABLE_LABEL);
        let _ = writeln!(out, "    .zero {}", STATE_SIZE - 8);
        let _ = writeln!(out, ".section .text");
    });
    out
}

// ------------------------------------------------------------------- Runtime

/// The collector runtime as readable Firn (`lib/gc/gc.fi`), embedded.
const RUNTIME: &str = include_str!("../../lib/gc/gc.fi");

/// **Round 53** — the collections of variable length (`SPEC` §3.5.2).
///
/// They live in FILES OF THEIR OWN and are appended to `gc.fi`: together
/// they are one module, so they see its names (`KOPF`, `F_SLOTS`,
/// `__gc_alloc_raw`, `__gc_barrier`) without an `import`. They are appended
/// only when the program really needs them — a program with `gc class` but
/// without collections produces the same code afterwards as before.
const RUNTIME_VEC: &str = include_str!("../../lib/gc/gcvec.fi");
const RUNTIME_MAP: &str = include_str!("../../lib/gc/gcmap.fi");

/// **ROUND 88** — the name of the setup the two code generators write into
/// `_start` when the runtime is part of the program. It lies in the root
/// namespace like everything in `lib/gc/gc.fi`.
pub(crate) const FN_INIT: &str = "gc_init";

/// Path name of the runtime pulled in, for error messages and `.debug_line`.
pub(crate) const RUNTIME_PATH: &str = "lib/gc/gc.fi";

/// Does this program need the runtime? Decided on the token stream: somewhere
/// the two identifiers `gc class` stand next to each other.
pub(crate) fn source_needs_gc(toks: &[crate::lexer::Token]) -> bool {
    if toks.windows(2).any(|w| {
        matches!(&w[0].kind, TokKind::Ident(a) if a == "gc")
            && matches!(&w[1].kind, TokKind::Ident(b) if b == "class")
    }) {
        return true;
    }
    // Round 58: `gc fn(…)` — a capturing closure. Its record is a GC
    // object, so the same runtime has to be there. Two tokens are enough to
    // see it, and a plain `fn(…)` closure (which captures nothing and
    // allocates nothing) deliberately does NOT pull the collector in.
    if toks.windows(2).any(|w| {
        matches!(&w[0].kind, TokKind::Ident(a) if a == "gc")
            && matches!(&w[1].kind, TokKind::KwFn)
    }) {
        return true;
    }
    // ROUND 70: `str` is GC managed. `__str_eq` and `__str_concat` live in
    // this runtime, so a program that works with `str` pulls it in — the
    // signal is read off the tokens, as conservatively as everything else
    // here (strtype.rs::source_uses_str).
    if crate::strtype::source_uses_str(toks) {
        return true;
    }
    // Round 49: the thread runtime lives in the same file — it needs the
    // same static state block, and the collector needs it. A program with
    // threads but without `gc class` therefore pulls it in through its
    // identifiers.
    toks.iter().any(|t| match &t.kind {
        TokKind::Ident(a) => a.starts_with("thread_") || a.starts_with("__thread"),
        _ => false,
    })
}

/// Does the source declare `error AllocError { … }` itself already?
pub(crate) fn source_has_allocerror(toks: &[crate::lexer::Token]) -> bool {
    toks.windows(2).any(|w| {
        matches!(&w[0].kind, TokKind::KwError)
            && matches!(&w[1].kind, TokKind::Ident(b) if b == ERR_SET)
    })
}

/// **Round 53** — does this program need the collections (`GcVec`/`GcMap`)?
///
/// Decided on the token stream like `source_needs_gc`: somewhere there is an
/// identifier that begins with `GcVec`, `GcMap`, `gcvec_` or `gcmap_`.
/// The prefix test instead of an equality test covers both — the type
/// `GcVec[Gc[T]]` and the call `gcvec_append[T](…)`.
pub(crate) fn source_needs_collections(toks: &[crate::lexer::Token]) -> bool {
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

/// Does the root file declare `fn __gc_finalize` itself?
pub(crate) fn source_has_finalizer(toks: &[crate::lexer::Token]) -> bool {
    toks.windows(2).any(|w| {
        matches!(&w[0].kind, TokKind::KwFn)
            && matches!(&w[1].kind, TokKind::Ident(b) if b == FN_FINAL)
    })
}

/// Does the source declare `fn __thread_work` itself already?
pub(crate) fn source_has_thread_work(toks: &[crate::lexer::Token]) -> bool {
    toks.windows(2).any(|w| {
        matches!(&w[0].kind, TokKind::KwFn)
            && matches!(&w[1].kind, TokKind::Ident(b) if b == FN_THREAD)
    })
}

/// The empty default of the thread dispatcher.
fn thread_work_default() -> String {
    let mut s = String::new();
    s.push_str("// Round 49: default of the thread dispatcher. The program\n");
    s.push_str("// declares none of its own, so a thread does nothing.\n");
    s.push_str("fn ");
    s.push_str(FN_THREAD);
    s.push_str("(kind: u64, arg: u64) -> u64 {\n");
    s.push_str("    return kind + arg - kind - arg\n");
    s.push_str("}\n");
    s
}

/// The empty default of the finalizer dispatcher.
fn finalizer_default() -> String {
    let mut s = String::new();
    s.push_str("// Round 47: default of the finalizer dispatcher. The program\n");
    s.push_str("// declares none of its own, so cleanup does nothing.\n");
    s.push_str("fn ");
    s.push_str(FN_FINAL);
    s.push_str("(kind: u64, p: *mut u8) {\n");
    s.push_str("    let _unused: u64 = kind + (p as u64)\n");
    s.push_str("}\n");
    s
}

thread_local! {
    /// Was the runtime pulled into this program? (Round 49: then the state
    /// block has to appear in the assembler even without `gc class`.)
    static RUNTIME_INSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Is the collector/thread runtime part of this program?
pub(crate) fn runtime_active() -> bool {
    RUNTIME_INSIDE.with(|c| c.get())
}

/// Source text of the runtime. `with_error_set = false` when the program
/// declares `AllocError` itself already (error set names are program wide).
/// `with_finalizer = false` when the root file brings the dispatcher along
/// itself.
pub(crate) fn runtime_source(
    with_error_set: bool,
    with_finalizer: bool,
    with_thread_work: bool,
    with_collections: bool,
) -> String {
    let mut s = String::new();
    if with_error_set {
        s.push_str("error AllocError { OutOfMemory }\n");
    } else {
        s.push_str("// AllocError is declared by the program itself\n");
    }
    if with_finalizer {
        s.push_str(&finalizer_default());
    } else {
        s.push_str("// __gc_finalize is declared by the program itself\n");
    }
    if with_thread_work {
        s.push_str(&thread_work_default());
    } else {
        s.push_str("// __thread_work is declared by the program itself\n");
    }
    s.push_str(RUNTIME);
    if with_collections {
        s.push_str(RUNTIME_VEC);
        s.push_str(RUNTIME_MAP);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_names_are_gc_allocations() {
        assert!(is_gc_alloc_call("gc Node"));
        assert!(!is_gc_alloc_call("gcNode"));
        assert!(is_gc_alloc_call("gc_collect"));
        assert!(is_gc_alloc_call("dom__gc_collect"));
        assert!(!is_gc_alloc_call("gc_collectx"));
        assert!(!is_gc_alloc_call("tokenize"));
    }

    #[test]
    fn without_classes_is_no_ty_in_gc_ptr() {
        hook_reset();
        assert!(!is_gc_ref(&Type::ptr(Type::U8, true)));
        assert!(!is_gc_ref(&Type::U64));
        assert!(!has_classes());
    }

    #[test]
    fn runtime_contains_the_required_names() {
        let q = runtime_source(true, true, true, true);
        for n in ["gc_init", "gc_collect", "gc_live_objects", FN_ALLOC, FN_WEAK, FN_STRONG, FN_AS] {
            assert!(q.contains(n), "runtime without '{}'", n);
        }
        assert!(q.contains("error AllocError"));
        assert!(!runtime_source(false, true, true, false).contains("error AllocError {"));
        // Round 47: the dispatcher is there exactly ONCE — either as the
        // default or from the program, never twice.
        assert!(q.contains("fn __gc_finalize(kind: u64, p: *mut u8) {"));
        assert!(!runtime_source(true, false, true, false).contains("fn __gc_finalize(kind: u64, p: *mut u8) {"));
        // Round 49: the same for the thread dispatcher.
        assert!(!runtime_source(true, true, false, false).contains("fn __thread_work(kind: u64, arg: u64) -> u64 {"));
        assert!(q.contains("gc_finalizer_set"));
        // Round 53: the collections join only when they are needed.
        for n in ["gcvec_append", "gcmap_set", "gc class GcSlots"] {
            assert!(q.contains(n), "runtime without '{}'", n);
            assert!(
                !runtime_source(true, true, true, false).contains(n),
                "'{}' present even without collections",
                n
            );
        }
        assert!(q.contains("gc_root_register"));
    }
}
