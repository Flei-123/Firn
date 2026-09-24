// SPDX-License-Identifier: MPL-2.0
//! **Round GAPS** — arithmetic on typed pointers.
//!
//! ```firn
//! p + n      // *T + integer  -> *T, n ELEMENTS further (n * size_of(T) octets)
//! p - n      // *T - integer  -> *T, n elements back
//! p - q      // *T - *T       -> i64, the distance in ELEMENTS
//! ```
//!
//! Until this round every one of these was written by hand through an
//! integer: `((p as usize) + 8) as *mut i32` (certus `anim/mix.fi:298`,
//! `css/cascade.fi:2297`, docs/LUECKEN.md B13). The `8` is the element size
//! done in the head -- change the element type and the offset is silently
//! wrong. The operator does the multiplication itself.
//!
//! The rules are C's, and so is the lack of a check: a pointer is already
//! the unchecked corner of the language (SPEC 3.4), `p + n` checks neither
//! the bounds nor an overflow of the address, on every build level. `n` may
//! be any concrete integer type (signed ones may go backwards); an untyped
//! literal becomes `usize`. `n + p` is not accepted -- the pointer comes
//! first, as in every place the Certus and OrientOS code does it by hand.
//! `p - q` needs two pointers to the same element type; the octet distance
//! is divided exactly (the pointers are assumed to lie in one array).
//!
//! In FIR it is the same `PtrAdd` an index expression produces
//! (`layout.rs::elem_addr`) -- every pass already knows it.

use crate::ast::{BinOp, Expr};
use crate::diag::Span;
use crate::fir::{BinOp as FBin, FTy, Op, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

fn pointee(t: &Type) -> Option<&Type> {
    match t {
        Type::Ptr { inner, .. } => Some(inner),
        _ => None,
    }
}

/// Hook from `sema::binary`. `None` = not pointer arithmetic, the ordinary
/// check goes on.
pub(crate) fn hook_binary(
    ck: &mut Checker,
    op: BinOp,
    l: &Expr,
    r: &Expr,
    espan: Span,
) -> Option<Type> {
    if !matches!(op, BinOp::Add | BinOp::Sub) {
        return None;
    }
    let lp = ck.probe(l)?;
    // A `Gc[T]` is a pointer too, but it points at ONE object the collector
    // owns; there is no neighbour to move to.
    if !lp.is_ptr() || crate::gc::is_gc_ptr(&lp) {
        return None;
    }
    let lt = ck.expr(l, None);
    if lt.is_error() {
        ck.type_out_expr(r);
        return Some(Type::Error);
    }
    // `p - q`: the distance of two pointers.
    if op == BinOp::Sub && ck.probe(r).map(|t| t.is_ptr()).unwrap_or(false) {
        let rt = ck.expr(r, None);
        if rt.is_error() {
            return Some(Type::Error);
        }
        let same = match (pointee(&lt), pointee(&rt)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        if !same {
            ck.dg.error(
                espan,
                format!(
                    "'-' between two pointers needs the same element type, found {} and {}",
                    ck.tcx.name_of(&lt),
                    ck.tcx.name_of(&rt)
                ),
            );
            return Some(Type::Error);
        }
        if pointee_size(ck, &lt) == 0 {
            ck.dg.error(espan, "pointer arithmetic on a pointer to a type without a size");
            return Some(Type::Error);
        }
        return Some(Type::I64);
    }
    let rt = ck.expr(r, Some(&Type::Usize));
    if rt.is_error() {
        return Some(Type::Error);
    }
    if !rt.is_concrete_int() {
        ck.dg.error_note(
            espan,
            format!(
                "operator '{}' on a pointer expects an integer on the right, found {}",
                if op == BinOp::Add { "+" } else { "-" },
                ck.tcx.name_of(&rt)
            ),
            "p + n moves n elements; the other side has to be an integer",
        );
        return Some(Type::Error);
    }
    if pointee_size(ck, &lt) == 0 {
        ck.dg.error(espan, "pointer arithmetic on a pointer to a type without a size");
        return Some(Type::Error);
    }
    Some(lt)
}

fn pointee_size(ck: &Checker, t: &Type) -> u64 {
    match pointee(t) {
        Some(inner) => ck.tcx.size_of(inner),
        None => 0,
    }
}

/// Hook from `lower::lower_binary`. `None` = not pointer arithmetic.
pub(crate) fn lower_binary(lw: &mut Lower, op: BinOp, a: &Expr, b: &Expr) -> Option<Option<Val>> {
    if !matches!(op, BinOp::Add | BinOp::Sub) {
        return None;
    }
    let at = lw.ty_of(a);
    if crate::gc::is_gc_ptr(&at) {
        return None;
    }
    let size = match pointee(&at) {
        Some(inner) => lw.size_align(inner).0,
        None => return None,
    };
    let bt = lw.ty_of(b);
    if bt.is_ptr() {
        // p - q = (p - q) / size, in i64
        return Some((|| {
            let pv = lw.lower_expr(a)?;
            let qv = lw.lower_expr(b)?;
            let pi = lw.push(FTy::I64, Op::Cast { src: pv, from: FTy::Ptr });
            let qi = lw.push(FTy::I64, Op::Cast { src: qv, from: FTy::Ptr });
            let d = lw.push(FTy::I64, Op::Bin(FBin::Sub, pi, qi));
            if size == 1 {
                return Some(d);
            }
            let sz = lw.constant(FTy::I64, size as i128);
            Some(lw.push(FTy::I64, Op::Bin(FBin::Div, d, sz)))
        })());
    }
    Some((|| {
        let base = lw.lower_expr(a)?;
        let idx_ty = lw.fty_of(b)?;
        let idx = lw.lower_expr(b)?;
        Some(move_by(lw, base, size, idx, idx_ty, op == BinOp::Sub))
    })())
}

/// Is `t` a pointer the operators move? (every `*T` / `*mut T`, not `Gc[T]`)
pub(crate) fn is_arith_ptr(t: &Type) -> bool {
    t.is_ptr() && !crate::gc::is_gc_ptr(t)
}

/// The element size behind a pointer type in the lowering.
pub(crate) fn elem_size(lw: &Lower, t: &Type) -> u64 {
    match pointee(t) {
        Some(inner) => lw.size_align(inner).0,
        None => 1,
    }
}

/// `base` moved `idx` elements of `size` octets forward (or back).
pub(crate) fn move_by(lw: &mut Lower, base: Val, size: u64, idx: Val, idx_ty: FTy, back: bool) -> Val {
    let idx64 = if idx_ty == FTy::U64 {
        idx
    } else {
        lw.push(FTy::U64, Op::Cast { src: idx, from: idx_ty })
    };
    let idx64 = if back {
        let z = lw.constant(FTy::U64, 0);
        lw.push(FTy::U64, Op::Bin(FBin::Sub, z, idx64))
    } else {
        idx64
    };
    lw.elem_addr(base, size, idx64, FTy::U64)
}

/// Hook from `sema` for `p += n` / `p -= n` (`Stmt::AssignOp`): `true` if
/// it was pointer arithmetic and has been checked.
pub(crate) fn check_assign_op(ck: &mut Checker, op: BinOp, ty: &Type, value: &Expr, span: Span) -> bool {
    if !matches!(op, BinOp::Add | BinOp::Sub) || !is_arith_ptr(ty) {
        return false;
    }
    let rt = ck.expr(value, Some(&Type::Usize));
    if rt.is_error() {
        return true;
    }
    if !rt.is_concrete_int() {
        ck.dg.error_note(
            span,
            format!(
                "operator '{}=' on a pointer expects an integer on the right, found {}",
                if op == BinOp::Add { "+" } else { "-" },
                ck.tcx.name_of(&rt)
            ),
            "p += n moves n elements; the other side has to be an integer",
        );
    } else if pointee_size(ck, ty) == 0 {
        ck.dg.error(span, "pointer arithmetic on a pointer to a type without a size");
    }
    true
}
