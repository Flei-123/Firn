// SPDX-License-Identifier: MPL-2.0
//! **Round GAPS** — reading a float's bit pattern, and back, without a detour
//! through memory.
//!
//! ```firn
//! __bits(x: f64) -> u64            // the IEEE 754 pattern, unchanged
//! __bits(x: f32) -> u32
//! __f64_from_bits(u: u64) -> f64   // the way back
//! __f32_from_bits(u: u32) -> f32
//! ```
//!
//! Until this round every NaN-boxing value, every hash of a number, every
//! `frexp`/`ldexp` and every binary format writer stored the float into a
//! stack slot and read it back through a pointer of the other type
//! (`lib/std/core.fi`, certus `js/builtin2.fi`, `browser/netjs.fi`). That
//! works, but it is three instructions and a memory round trip where there
//! should be none, and it defeats mem2reg for the slot.
//!
//! In FIR it is `Op::Un(UnOp::Bits, x)`: the instruction type is the TARGET
//! type, the operand keeps its own type, both have the same width. Every
//! float already lives in its frame slot as its bit pattern, so the backends
//! only copy the word (and clear the upper half for the 32-bit forms).
//! Constant folding is the identity on the pattern.

use crate::ast::Expr;
use crate::diag::Span;
use crate::fir::{FTy, Op, UnOp, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

pub(crate) const BITS: &str = "__bits";
pub(crate) const F64_FROM: &str = "__f64_from_bits";
pub(crate) const F32_FROM: &str = "__f32_from_bits";

pub(crate) fn is_bits_call(name: &str) -> bool {
    name == BITS || name == F64_FROM || name == F32_FROM
}

/// Hook from `sema::call`. `None` if this is not one of the primitives or if
/// the program declares a function of the same spelling -- that one wins.
pub(crate) fn hook_call(ck: &mut Checker, name: &str, args: &[Expr], espan: Span) -> Option<Type> {
    if !is_bits_call(name) || ck.fns.contains_key(name) {
        return None;
    }
    let form = match name {
        BITS => "__bits(x: f64) -> u64 (or f32 -> u32)",
        F64_FROM => "__f64_from_bits(u: u64) -> f64",
        _ => "__f32_from_bits(u: u32) -> f32",
    };
    if args.len() != 1 {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error_note(
            espan,
            format!("'{}' expects exactly one argument, found {}", name, args.len()),
            format!("the form is {}", form),
        );
        return Some(Type::Error);
    }
    let (want, result) = match name {
        BITS => {
            let t = ck.expr(&args[0], None);
            return match t {
                Type::F64 => Some(Type::U64),
                Type::F32 => Some(Type::U32),
                Type::Error => Some(Type::Error),
                other => {
                    ck.dg.error_note(
                        args[0].span,
                        format!("'{}' expects an f64 or f32, found {}", BITS, ck.tcx.name_of(&other)),
                        format!("the form is {}", form),
                    );
                    Some(Type::Error)
                }
            };
        }
        F64_FROM => (Type::U64, Type::F64),
        _ => (Type::U32, Type::F32),
    };
    let t = ck.expr(&args[0], Some(&want));
    if t == Type::Error {
        return Some(Type::Error);
    }
    if t != want {
        ck.dg.error_note(
            args[0].span,
            format!(
                "'{}' expects a {}, found {}",
                name,
                ck.tcx.name_of(&want),
                ck.tcx.name_of(&t)
            ),
            format!("the form is {}; convert first with 'as'", form),
        );
        return Some(Type::Error);
    }
    Some(result)
}

/// Hook from `lower::lower_call`.
pub(crate) fn lower_call(lo: &mut Lower, name: &str, args: &[Expr], span: Span) -> Option<Option<Val>> {
    if args.len() != 1 {
        return lo.ice(span, "bit reinterpretation with wrong arity");
    }
    let to = match name {
        BITS => match lo.ty_of(&args[0]) {
            Type::F32 => FTy::U32,
            _ => FTy::U64,
        },
        F64_FROM => FTy::F64,
        _ => FTy::F32,
    };
    let x = lo.lower_expr(&args[0])?;
    Some(Some(lo.push(to, Op::Un(UnOp::Bits, x))))
}
