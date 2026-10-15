// SPDX-License-Identifier: MPL-2.0
//! **`size_of[T]()`** — the size of a type as bytes, at compile time.
//!
//! INTERFACE (fixed):
//!   `pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr>`  (Parser)
//!   `pub(crate) fn hook_call(..) -> Option<Type>`                 (type checker)
//!   `pub(crate) fn value(..) -> Option<i128>`                     (Lowering)
//!
//! ## What for
//!
//! `docs/SELF_HOSTING.md` §4 lists `Vec[T]` as the second largest blocker
//! on the way to stage 1. A field of **fixed** size works since round 2
//! (`tests/211_generic_struct.fi`), a **growing** one does not: for that the
//! address of the `i`-th element has to be computed, and that needs the
//! element size.
//!
//! ## How it looks
//!
//! ```firn
//! let n: usize = size_of[i32]()      // 4
//! let m: usize = size_of[Point]()    // layout of the struct
//! ```
//!
//! ## How it is built
//!
//! Like `gc_null[C]()` (see `gc.rs`): the parser spots the form directly and
//! wraps it as a **call carrying a reserved identifier** that holds the type
//! text. The type checker resolves the type, computes the size and keeps it;
//! lowering substitutes a constant. At run time **nothing** of it is left
//! over.
//!
//! Routing it through the name is deliberate: `size_of` is thereby no
//! keyword and collides with no identifier that somebody already uses.

use crate::ast::{Expr, ExprKind, TypeExpr};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::Type;
use std::cell::RefCell;
use std::collections::HashMap;

/// Reserved identifier prefix. Firn identifiers cannot contain a `$`, so a
/// collision with user code is ruled out.
const P_SIZE: &str = "size_of$";
/// r169: `has_gc[T]()` -- compile-time `bool`: does a `T` hold GC pointers?
const P_HASGC: &str = "has_gc$";
/// r169: `gc_track[T](p, bytes)` / `gc_untrack[T](p)` -- register / drop a
/// buffer as a collector root, but ONLY when `T` holds GC pointers (and the
/// collector runtime is part of the program); otherwise they vanish.
const P_TRACK: &str = "gc_track$";
const P_UNTRACK: &str = "gc_untrack$";
const PREFIXES: [&str; 4] = [P_SIZE, P_HASGC, P_TRACK, P_UNTRACK];

/// Is `name` one of the reserved intrinsic call names of this module?
pub(crate) fn is_intrinsic(name: &str) -> bool {
    PREFIXES.iter().any(|p| name.starts_with(p))
}

/// The prefix of `name` (with its `$`), if it is one of ours.
pub(crate) fn prefix_of(name: &str) -> Option<&'static str> {
    PREFIXES.iter().copied().find(|p| name.starts_with(p))
}

thread_local! {
    /// Identifier -> size. Filled by the type checker, read by lowering.
    static VALUES: RefCell<HashMap<String, i128>> = RefCell::new(HashMap::new());
    /// Round TUPLES: the type arguments that are no plain name (`(i32, i32)`,
    /// `*mut u8`) which `size_of[T]()` met inside a template. The call name
    /// can only carry a name, so it carries `#<index>` into this list.
    static TYPES: RefCell<Vec<TypeExpr>> = RefCell::new(Vec::new());
}

/// Resets the table (one per compilation, `parser::reset_hooks`).
pub(crate) fn hook_reset() {
    VALUES.with(|w| w.borrow_mut().clear());
    TYPES.with(|w| w.borrow_mut().clear());
}

/// Round TUPLES: the call name for `size_of[T]()` with `T` bound to a type
/// that is not a plain name (`mono.rs::subst_call_name`).
pub(crate) fn stash_type(prefix: &str, te: &TypeExpr) -> String {
    let idx = TYPES.with(|t| {
        let mut t = t.borrow_mut();
        t.push(te.clone());
        t.len() - 1
    });
    format!("{}#{}", prefix, idx)
}

/// `// HOOK sizeof` in `parser.rs::primary` -- `size_of[T]()`, `has_gc[T]()`,
/// `gc_track[T](p, n)`, `gc_untrack[T](p)`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    let (prefix, nargs): (&str, usize) = match p.kind() {
        TokKind::Ident(n) if n == "size_of" => (P_SIZE, 0),
        TokKind::Ident(n) if n == "has_gc" => (P_HASGC, 0),
        TokKind::Ident(n) if n == "gc_track" => (P_TRACK, 2),
        TokKind::Ident(n) if n == "gc_untrack" => (P_UNTRACK, 1),
        _ => return None,
    };
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::LBracket)) {
        return None;
    }
    let what = prefix.trim_end_matches('$').to_string();
    let start = p.bump(); // the intrinsic name
    p.bump(); // '['
    // DELIBERATELY JUST ONE TYPE NAME, no full type expression:
    // `size_of[i32]`, `size_of[Point]`. Whoever needs the size of a composite
    // type gives it a name — which reads better anyway than `size_of[*mut u8]`.
    let (ty_name, ty_span) = p.ident(&format!("after '{}['", what))?;
    // HOOK alias: `size_of[Idx]()` with `type Idx = u32` is `size_of[u32]()`
    let ty_name = crate::alias::hook_plain_name(p, ty_name, ty_span);
    if !p.expect(TokKind::RBracket, &format!("after the type argument of '{}'", what)) {
        return None;
    }
    if !p.expect(TokKind::LParen, &format!("after the type argument of '{}'", what)) {
        return None;
    }
    let (args, end) = if nargs == 0 {
        match p.kind() {
            TokKind::RParen => (Vec::new(), p.bump()),
            _ => {
                p.error_here(format!("'{}' takes no arguments", what));
                return None;
            }
        }
    } else {
        p.call_args(&format!("after the arguments of '{}'", what))
    };
    let span = Parser::join(start, end);
    // The type text moves into the name; it is resolved in the type checker,
    // which knows the struct table.
    Some(p.mk(span, ExprKind::Call(format!("{}{}", prefix, ty_name), args, start)))
}

/// r169: does a value of type `t` hold pointers the collector must see?
/// `str` (its octets may live on the GC heap), `Gc[C]`, closures (`fn`
/// values may point at a GC record) and every struct / array that holds one.
/// Raw `*mut u8` / `u64` are NOT counted: the compiler cannot know what they
/// hold. `GcWeak` is deliberately not a root.
pub(crate) fn type_has_gc(ck: &Checker, t: &Type) -> bool {
    fn walk(ck: &Checker, t: &Type, seen: &mut Vec<usize>, depth: u32) -> bool {
        if depth > 16 {
            return false;
        }
        if crate::gc::is_gc_ptr(t) {
            return true;
        }
        match t {
            Type::Fn { .. } => true,
            Type::Array(e, _) => walk(ck, e, seen, depth + 1),
            Type::Struct(i) => {
                if crate::strtype::is_str(t) {
                    return true;
                }
                if crate::gc::is_gc_weak(t) || seen.contains(i) {
                    return false;
                }
                seen.push(*i);
                let fields: Vec<Type> = match ck.tcx.structs.get(*i) {
                    Some(sd) => sd.fields.iter().map(|f| f.ty.clone()).collect(),
                    None => Vec::new(),
                };
                fields.iter().any(|f| walk(ck, f, seen, depth + 1))
            }
            _ => false,
        }
    }
    walk(ck, t, &mut Vec::new(), 0)
}

/// r582: the pointer-word raster of one `T` element of a `gc_track[T]` buffer.
/// The collector scans a root buffer conservatively word by word; words that
/// can never hold a heap pointer (`str.len`, `f32`/`f64`/`bool`/`u8..u32`,
/// padding) need not be looked at. Result: 0 = scan every word (also for every
/// type that is not word-aligned, bigger than 32 words, or has no gap), else
/// `stride_in_words << 32 | mask` (bit k = word k of an element may hold a
/// pointer). `u64`/`usize`/`i64`/`isize`, raw pointers, `Gc`, `fn` and `v128`
/// stay conservative: they can carry a heap pointer.
pub(crate) fn type_raster(ck: &Checker, t: &Type) -> u64 {
    fn mark(words: &mut [bool], off: u64, size: u64) -> bool {
        if off % 8 != 0 {
            return false;
        }
        let first = (off / 8) as usize;
        let last = ((off + size + 7) / 8) as usize;
        if last > words.len() {
            return false;
        }
        for w in &mut words[first..last] {
            *w = true;
        }
        true
    }
    fn walk(ck: &Checker, t: &Type, off: u64, words: &mut [bool], depth: u32) -> bool {
        if depth > 16 {
            return false;
        }
        match t {
            Type::I8 | Type::I16 | Type::I32 | Type::U8 | Type::U16 | Type::U32
            | Type::Bool | Type::F32 | Type::F64 | Type::Void => true,
            Type::I64 | Type::U64 | Type::Usize | Type::Isize | Type::Ptr { .. }
            | Type::Fn { .. } | Type::V128 => mark(words, off, ck.tcx.size_of(t)),
            Type::Array(e, n) => {
                let es = ck.tcx.size_of(e);
                for k in 0..*n {
                    if !walk(ck, e, off + k * es, words, depth + 1) {
                        return false;
                    }
                }
                true
            }
            Type::Struct(i) => {
                let sd = match ck.tcx.structs.get(*i) {
                    Some(sd) => sd,
                    None => return false,
                };
                let is_str = crate::strtype::is_str(t);
                for f in &sd.fields {
                    if is_str {
                        // only the octet pointer of a `str`; its length is a count
                        if f.ty.is_ptr() && !mark(words, off + f.offset, 8) {
                            return false;
                        }
                    } else if !walk(ck, &f.ty, off + f.offset, words, depth + 1) {
                        return false;
                    }
                }
                true
            }
            Type::UntypedInt | Type::Error => false,
        }
    }
    let size = ck.tcx.size_of(t);
    if size == 0 || size % 8 != 0 || size > 256 {
        return 0;
    }
    let mut words = vec![false; (size / 8) as usize];
    if !walk(ck, t, 0, &mut words, 0) {
        return 0;
    }
    if words.iter().all(|w| *w) || !words.iter().any(|w| *w) {
        return 0;
    }
    let mut mask: u64 = 0;
    for (k, w) in words.iter().enumerate() {
        if *w {
            mask |= 1 << k;
        }
    }
    ((words.len() as u64) << 32) | mask
}

/// `// HOOK sizeof` in `sema::call`.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Option<Type> {
    let prefix = prefix_of(name)?;
    let what = prefix.trim_end_matches('$');
    let ty_text = &name[prefix.len()..];
    let want = if prefix == P_TRACK {
        2
    } else if prefix == P_UNTRACK {
        1
    } else {
        0
    };
    if args.len() != want {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error(span, format!("'{}' takes {} argument(s)", what, want));
        return Some(Type::Error);
    }
    // Round TUPLES: `#<index>` names a stashed type expression
    let te = match ty_text.strip_prefix('#').and_then(|i| i.parse::<usize>().ok()) {
        Some(i) => match TYPES.with(|t| t.borrow().get(i).cloned()) {
            Some(te) => te,
            None => TypeExpr::Named(ty_text.to_string(), span),
        },
        None => TypeExpr::Named(ty_text.to_string(), span),
    };
    let t = ck.resolve_ty(&te);
    if t.is_error() {
        for a in args {
            ck.type_out_expr(a);
        }
        return Some(Type::Error);
    }
    if matches!(t, Type::Void) {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error(span, format!("'{}[void]' is not meaningful", what));
        return Some(Type::Error);
    }
    if prefix == P_SIZE {
        let size = ck.tcx.size_of(&t) as i128;
        VALUES.with(|w| w.borrow_mut().insert(name.to_string(), size));
        return Some(Type::Usize);
    }
    let has = type_has_gc(ck, &t);
    // r582: bit 0 = holds GC pointers; bits 8.. = the pointer-word raster
    let mut flag = has as i128;
    if has && prefix == P_TRACK {
        flag |= (type_raster(ck, &t) as i128) << 8;
    }
    VALUES.with(|w| w.borrow_mut().insert(name.to_string(), flag));
    if prefix == P_HASGC {
        return Some(Type::Bool);
    }
    // gc_track / gc_untrack: every argument is a `u64` or `usize` (same width)
    for a in args {
        let at = ck.expr(a, Some(&Type::U64));
        if !at.is_error() && at != Type::U64 && at != Type::Usize && !matches!(at, Type::UntypedInt) {
            ck.dg.error(a.span, format!("'{}' expects 'u64' or 'usize' arguments", what));
        }
    }
    Some(Type::Void)
}

/// r169: lowering of the intrinsics of this module. `None` = not ours.
/// `Some(r)` has the shape `lower_call` returns.
pub(crate) fn lower_hook(
    lo: &mut crate::lower::Lower,
    name: &str,
    args: &[Expr],
    _span: Span,
) -> Option<Option<Option<crate::fir::Val>>> {
    let prefix = prefix_of(name)?;
    let packed = VALUES.with(|w| w.borrow().get(name).copied()).unwrap_or(0);
    let flag = if prefix == P_SIZE { packed } else { packed & 1 };
    let rast = if prefix == P_TRACK { packed >> 8 } else { 0 };
    if prefix == P_SIZE {
        return Some(Some(Some(lo.constant(crate::fir::FTy::U64, flag))));
    }
    if prefix == P_HASGC {
        return Some(Some(Some(lo.constant(crate::fir::FTy::Bool, flag))));
    }
    // gc_track / gc_untrack: the arguments are always evaluated (order of
    // effects); the call is emitted only for a GC-holding element type AND
    // when the collector runtime is part of the program.
    let target = if prefix == P_TRACK { "gc_root_add" } else { "gc_root_del" };
    let mut vals = Vec::new();
    for a in args {
        match lo.lower_expr(a) {
            Some(v) => vals.push(v),
            None => return Some(None),
        }
    }
    if flag != 0 && lo.info.fns.contains_key(target) {
        if prefix == P_TRACK {
            // r582: the third argument of `gc_root_add` is the raster (0 = all words)
            vals.push(lo.constant(crate::fir::FTy::U64, rast));
        }
        lo.push_void(
            crate::fir::FTy::Void,
            crate::fir::Op::Call { name: target.to_string(), args: vals },
        );
    }
    Some(Some(None))
}

/// The size determined by the type checker — for lowering.
pub(crate) fn value(name: &str) -> Option<i128> {
    if !name.starts_with(P_SIZE) {
        return None;
    }
    VALUES.with(|w| w.borrow().get(name).copied())
}
