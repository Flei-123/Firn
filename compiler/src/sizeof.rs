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
pub(crate) fn stash_type(te: &TypeExpr) -> String {
    let idx = TYPES.with(|t| {
        let mut t = t.borrow_mut();
        t.push(te.clone());
        t.len() - 1
    });
    format!("{}#{}", P_SIZE, idx)
}

/// `// HOOK sizeof` in `parser.rs::primary` — `size_of[T]()`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    match p.kind() {
        TokKind::Ident(n) if n == "size_of" => {}
        _ => return None,
    }
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::LBracket)) {
        return None;
    }
    let start = p.bump(); // 'size_of'
    p.bump(); // '['
    // DELIBERATELY JUST ONE TYPE NAME, no full type expression:
    // `size_of[i32]`, `size_of[Point]`. Whoever needs the size of a composite
    // type gives it a name — which reads better anyway than `size_of[*mut u8]`.
    let (ty_name, ty_span) = p.ident("after 'size_of['")?;
    // HOOK alias: `size_of[Idx]()` with `type Idx = u32` is `size_of[u32]()`
    let ty_name = crate::alias::hook_plain_name(p, ty_name, ty_span);
    if !p.expect(TokKind::RBracket, "after the type argument of 'size_of'") {
        return None;
    }
    if !p.expect(TokKind::LParen, "after the type argument of 'size_of'") {
        return None;
    }
    let end = match p.kind() {
        TokKind::RParen => p.bump(),
        _ => {
            p.error_here("'size_of' takes no arguments".to_string());
            return None;
        }
    };
    let span = Parser::join(start, end);
    // The type text moves into the name; it is resolved in the type checker,
    // which knows the struct table.
    Some(p.mk(span, ExprKind::Call(format!("{}{}", P_SIZE, ty_name), Vec::new(), start)))
}

/// `// HOOK sizeof` in `sema::call`.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Option<Type> {
    let ty_text = name.strip_prefix(P_SIZE)?;
    if !args.is_empty() {
        ck.dg.error(span, "'size_of' takes no arguments".to_string());
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
        return Some(Type::Error);
    }
    if matches!(t, Type::Void) {
        ck.dg.error(span, "'size_of[void]' is not meaningful".to_string());
        return Some(Type::Error);
    }
    let size = ck.tcx.size_of(&t) as i128;
    VALUES.with(|w| w.borrow_mut().insert(name.to_string(), size));
    Some(Type::Usize)
}

/// The size determined by the type checker — for lowering.
pub(crate) fn value(name: &str) -> Option<i128> {
    if !name.starts_with(P_SIZE) {
        return None;
    }
    VALUES.with(|w| w.borrow().get(name).copied())
}
