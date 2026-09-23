// SPDX-License-Identifier: MPL-2.0
//! **Round GAPS** — `x as% T`, the narrowing conversion that is never
//! checked.
//!
//! ```firn
//! let lo: u8 = word as% u8          // the low eight bits, on every level
//! let s: i32 = bits as% i32         // u32 -> i32 by bit pattern
//! ```
//!
//! Since round 72 a narrowing `as` between two integer types is CHECKED on
//! dev-fast, release-safe and --no-opt (`Op::CheckedCast`): `300 as u8`
//! panics. That is right for a value that is supposed to fit -- and wrong
//! for the places where dropping the high bits IS the point: a hash, a
//! checksum, a pixel packed into a word, an `u32` bit pattern read as
//! `i32` (certus `paint/ico.fi:99`, docs/LUECKEN.md B12). Those had to be
//! written with masks (`(x & 255) as u8`) or not at all. `+% -% *%` exist
//! for exactly that reason on the arithmetic side (SPEC 13, item L9);
//! `as%` is the same promise for the conversion: the programmer says the
//! bits are meant to go.
//!
//! Only between two integer types -- for everything else `as` has no check
//! to switch off, and `as%` there would promise something it cannot keep.
//!
//! In the tree it is `__as_wrap(x as T)`: the parser wraps the ordinary
//! cast, so every pass that walks a cast sees one; only the lowering of
//! THIS call leaves the check out. A constant expression treats it like
//! `as` (the constant evaluator has always truncated).

use crate::ast::{Expr, ExprKind};
use crate::diag::Span;
use crate::fir::{Op, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

/// The name of the wrapper call the parser builds.
pub(crate) const NAME: &str = "__as_wrap";

pub(crate) fn is_wrap_call(name: &str) -> bool {
    name == NAME
}

thread_local! {
    /// The ids of the casts `as%` wraps, while they are being checked.
    static WRAPPING: std::cell::RefCell<Vec<crate::ast::ExprId>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Asked by the `Cast` arm of `sema::expr_inner`: is this cast an `as%`?
/// Then an untyped literal inside must NOT take the target type (it would
/// not fit -- `0x1234 as% u8` is the point), it gets the widest one.
pub(crate) fn literal_hint(cast_id: crate::ast::ExprId, inner: &Expr) -> Option<Type> {
    if !WRAPPING.with(|w| w.borrow().contains(&cast_id)) {
        return None;
    }
    match &inner.kind {
        ExprKind::Unary(crate::ast::UnOp::Neg, _) => Some(Type::I64),
        _ => Some(Type::U64),
    }
}

/// Hook from `sema::call`.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    espan: Span,
) -> Option<Type> {
    if !is_wrap_call(name) || ck.fns.contains_key(name) {
        return None;
    }
    let inner = match args {
        [a] => match &a.kind {
            ExprKind::Cast(inner, _) => Some(inner),
            _ => None,
        },
        _ => None,
    };
    let inner = match inner {
        Some(i) => i,
        None => {
            for a in args {
                ck.type_out_expr(a);
            }
            ck.dg.error_note(
                espan,
                format!("'{}' is not called by hand", NAME),
                "write 'x as% T' -- the unchecked narrowing conversion",
            );
            return Some(Type::Error);
        }
    };
    WRAPPING.with(|w| w.borrow_mut().push(args[0].id));
    let dst = ck.expr(&args[0], None);
    WRAPPING.with(|w| {
        w.borrow_mut().pop();
    });
    if dst.is_error() {
        return Some(Type::Error);
    }
    let src = ck.expr_types.get(inner.id as usize).cloned().unwrap_or(Type::Error);
    if src.is_error() {
        return Some(Type::Error);
    }
    if !src.is_concrete_int() || !dst.is_concrete_int() {
        ck.dg.error_note(
            espan,
            format!(
                "'as%' converts between integer types only, found {} as% {}",
                ck.tcx.name_of(&src),
                ck.tcx.name_of(&dst)
            ),
            "'as%' switches off the range check of a narrowing 'as'; other conversions have none",
        );
        return Some(Type::Error);
    }
    Some(dst)
}

/// Hook from `lower::lower_call`: the cast WITHOUT the range check.
pub(crate) fn lower_call(lo: &mut Lower, args: &[Expr], span: Span) -> Option<Option<Val>> {
    let (cast, inner) = match args {
        [a] => match &a.kind {
            ExprKind::Cast(inner, _) => (a, inner),
            _ => return lo.ice(span, "__as_wrap without a cast"),
        },
        _ => return lo.ice(span, "__as_wrap with wrong arity"),
    };
    let from = lo.out_fty(inner)?;
    let to = lo.fty_of(cast)?;
    let src = lo.lower_expr(inner)?;
    if from == to {
        return Some(Some(src));
    }
    Some(Some(lo.push(to, Op::Cast { src, from })))
}
