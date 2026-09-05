// SPDX-License-Identifier: GPL-2.0-only
//! Atomic read-modify-write operation (round 47) — the foundation of
//! `Arc[T]` (SPEC §3.4: "`Arc[T]` is the thread-safe variant (atomic
//! counter). A separate type, so that single-threaded code does not pay for
//! the atomic counter.").
//!
//! Exactly ONE primitive, deliberately the smallest one that suffices:
//!
//! ```firn
//! __atomic_add(p: *mut u64, delta: u64) -> u64   // yields the OLD value
//! ```
//!
//! It turns into a single machine instruction `lock xadd qword ptr [..], r`.
//! That lets a counter go up (result irrelevant) and equally go down while
//! telling whether you were the last holder (old value == 1) — reference
//! counting needs nothing more. Subtraction is the addition of the two's
//! complement; a primitive of its own for that would be dead weight.
//!
//! **Why a dedicated path all the way down to the code generator rather than
//! plain `*p = *p + d`?** Because that is three instructions (`load`, `add`,
//! `store`) and between them another thread can do the very same — exactly
//! the lost increment that `Arc` exists against. Firn has no threads at
//! stage 0 (SPEC §7), so the difference is NOT measurable today by a
//! two-thread run; it is provable at the emitted instruction
//! (`tools/atomic/run.sh` reads the assembler and demands the `lock` prefix).
//! `docs/ROUND47.md` says the same — no "thread-safe" without proof.
//!
//! **Not part of this:** memory orderings (`acquire`/`release`/`relaxed`),
//! compare-and-swap (`compare_exchange`), atomic loads/stores of smaller
//! widths. `lock xadd` carries full ordering on x86-64 anyway; the finer
//! models belong to the round that brings threads.

use crate::ast::Expr;
use crate::diag::Span;
use crate::fir::{FTy, Op, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

/// Identifier of the primitive in the source text.
pub(crate) const ADD: &str = "__atomic_add";

/// Is this spelling the name of the builtin primitive?
pub(crate) fn is_atomic_call(name: &str) -> bool {
    name == ADD
}

// ----------------------------------------------------------------- Type phase

/// Hook from `sema::call`. `None` if this is not the primitive or if the
/// program contains a function of the same spelling — that one wins then.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if !is_atomic_call(name) || ck.fns.contains_key(name) {
        return None;
    }
    if args.len() != 2 {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error_note(
            espan,
            format!(
                "'{}' expects exactly two arguments (pointer, addend), found {}",
                ADD,
                args.len()
            ),
            "the form is __atomic_add(p: *mut u64, delta: u64) -> u64",
        );
        return Some(Type::Error);
    }
    let pt = ck.expr(&args[0], Some(&Type::ptr(Type::U64, true)));
    let dt = ck.expr(&args[1], Some(&Type::U64));
    if !pt.is_error() && !is_u64_ptr(&pt) {
        ck.dg.error_note(
            args[0].span,
            format!(
                "'{}' expects a *mut u64 as first argument, found {}",
                ADD,
                ck.tcx.name_of(&pt)
            ),
            "exactly one 64-bit word is changed atomically",
        );
        return Some(Type::Error);
    }
    if !dt.is_error() && !fits_as_u64(&dt) {
        ck.dg.error(
            args[1].span,
            format!(
                "'{}' expects a u64 as second argument, found {}",
                ADD,
                ck.tcx.name_of(&dt)
            ),
        );
        return Some(Type::Error);
    }
    let _ = nspan;
    Some(Type::U64)
}

fn is_u64_ptr(t: &Type) -> bool {
    match t {
        Type::Ptr { inner, .. } => **inner == Type::U64,
        _ => false,
    }
}

fn fits_as_u64(t: &Type) -> bool {
    matches!(t, Type::U64 | Type::UntypedInt)
}

// ------------------------------------------------------------ Lowering phase

/// Hook from `lower::lower_call`.
pub(crate) fn lower_atomic_call(lo: &mut Lower, name: &str, args: &[Expr], span: Span) -> Option<Option<Val>> {
    let _ = name;
    if args.len() != 2 {
        return lo.ice(span, "atomic primitive with wrong arity");
    }
    let p = lo.lower_expr(&args[0])?;
    let d = lo.lower_expr(&args[1])?;
    Some(Some(lo.push(FTy::U64, Op::AtomicAdd { addr: p, val: d })))
}
