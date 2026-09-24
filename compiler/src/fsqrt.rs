// SPDX-License-Identifier: MPL-2.0
//! **Round GAPS** — the square root as ONE machine instruction.
//!
//! ```firn
//! __sqrt(x: f64) -> f64      // sqrtsd on x86-64, fsqrt d on aarch64
//! __sqrt(x: f32) -> f32      // sqrtss / fsqrt s
//! ```
//!
//! Until this round Firn had no word for it. `lib/std/core.fi::sqrt` ran
//! Newton's iteration until the value stopped moving -- one f64 division per
//! step, ~6 to 60 steps depending on the magnitude, never correctly rounded
//! for every input, and an endless loop for `inf` and `NaN` (`NaN != NaN`
//! holds forever). Certus' painter wrote its own (`fsqrt2`, "about ten f64
//! divisions PER PIXEL", lib/paint/painter.fi) and then a second one that
//! reuses the left neighbour's root as its seed, just to get under that
//! cost; osum's WASM interpreter carries a third (kernel/app/wasm.fi).
//!
//! The instruction is exact by IEEE 754 (correctly rounded, `sqrt(-0) =
//! -0`, `sqrt(x<0) = NaN`, `sqrt(inf) = inf`) and costs a few cycles of
//! latency instead of a loop.
//!
//! In FIR it is `Op::Un(UnOp::Sqrt, x)` on a float type -- a unary
//! operation like `neg`, so every pass that already walks `Op::Un` (copy
//! propagation, CSE, LICM, inlining, mem2reg) handles it without a special
//! case. Constant folding leaves it alone like every float operation
//! (`opt.rs::op_has_float`).

use crate::ast::Expr;
use crate::diag::Span;
use crate::fir::{FTy, Op, UnOp, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

/// Identifier of the primitive in the source text.
pub(crate) const SQRT: &str = "__sqrt";

pub(crate) fn is_sqrt_call(name: &str) -> bool {
    name == SQRT
}

/// Hook from `sema::call`. `None` if this is not the primitive or if the
/// program declares a function of the same spelling -- that one wins then.
pub(crate) fn hook_call(ck: &mut Checker, name: &str, args: &[Expr], espan: Span) -> Option<Type> {
    if !is_sqrt_call(name) || ck.fns.contains_key(name) {
        return None;
    }
    if args.len() != 1 {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error_note(
            espan,
            format!("'{}' expects exactly one argument, found {}", SQRT, args.len()),
            "the form is __sqrt(x: f64) -> f64 (or f32 -> f32)",
        );
        return Some(Type::Error);
    }
    let t = ck.expr(&args[0], None);
    match t {
        Type::F64 | Type::F32 | Type::Error => Some(t),
        other => {
            ck.dg.error_note(
                args[0].span,
                format!("'{}' expects an f64 or f32, found {}", SQRT, ck.tcx.name_of(&other)),
                "convert integers first: __sqrt(n as f64)",
            );
            Some(Type::Error)
        }
    }
}

/// Hook from `lower::lower_call`.
pub(crate) fn lower_call(lo: &mut Lower, args: &[Expr], span: Span) -> Option<Option<Val>> {
    if args.len() != 1 {
        return lo.ice(span, "__sqrt with wrong arity");
    }
    let ty = lo.ty_of(&args[0]);
    let ft = match ty {
        Type::F32 => FTy::F32,
        _ => FTy::F64,
    };
    let x = lo.lower_expr(&args[0])?;
    Some(Some(lo.push(ft, Op::Un(UnOp::Sqrt, x))))
}
