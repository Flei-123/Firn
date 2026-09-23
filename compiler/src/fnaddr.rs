// SPDX-License-Identifier: MPL-2.0
//! **Round GAPS** — a function value as a number, and a number as a function
//! value.
//!
//! ```firn
//! let r: u64 = f as u64                  // the address of f's RECORD
//! let g: fn(i32) -> i32 = r as fn(i32) -> i32   // and back: g is f
//! let none: fn(i32) -> i32 = 0 as fn(i32) -> i32 // the null function
//! if (none as u64) == 0 { ... }          // "is there one?"
//! let code: u64 = __code_of(f)           // the machine code address
//! ```
//!
//! A function value is one word: the address of its record (word 0 = the
//! code address, the words after it = what a closure captured; ROUND58).
//! Until this round the only way to a number was `name as *T` on a DIRECTLY
//! NAMED function (round 75, a C callback). Everything else went through
//! memory: Certus' Android port stored the value into a struct field and
//! read the word back twice (`code_of4`, lib/android/a_main.fi), osum's
//! kernel symbol table did the same (`kernel/lib/ksym.fi`, "Firn hat keinen
//! Ausdruck, der eine Funktion in eine Zahl wandelt"), and every driver
//! table carried stub functions because there was no null (`fs/ext4.fi`,
//! `fs/vfsops.fi`: "Firn hat keinen Nullzeiger fuer `fn`").
//!
//! The rules:
//! * `f as u64` / `f as usize` is the RECORD address -- the identity of the
//!   value, so the way back `n as fn(..)` gives the same function. 0 is the
//!   null function; calling it faults like a null pointer does.
//! * `__code_of(f)` is the CODE address (word 0 of the record) -- what a C
//!   API, a JIT or a symbol table wants. For a closure with captures that
//!   code still expects its record; handing it to C is the caller's choice.
//! * The number is not a root for the collector: a closure that captured
//!   something and lives only as a number may be collected. Named functions
//!   and closures without captures live in `.rodata` and never move.

use crate::ast::Expr;
use crate::diag::Span;
use crate::fir::{FTy, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

pub(crate) const CODE_OF: &str = "__code_of";

pub(crate) fn is_code_of_call(name: &str) -> bool {
    name == CODE_OF
}

/// Is this cast one of the new function <-> integer conversions?
pub(crate) fn cast_allowed(src: &Type, dst: &Type) -> bool {
    let word = |t: &Type| matches!(t, Type::U64 | Type::Usize);
    (src.is_fn() && word(dst)) || (word(src) && dst.is_fn())
}

/// Hook from `sema::call`.
pub(crate) fn hook_call(ck: &mut Checker, name: &str, args: &[Expr], espan: Span) -> Option<Type> {
    if !is_code_of_call(name) || ck.fns.contains_key(name) {
        return None;
    }
    if args.len() != 1 {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error_note(
            espan,
            format!("'{}' expects exactly one argument, found {}", CODE_OF, args.len()),
            "the form is __code_of(f: fn(..) -> R) -> u64",
        );
        return Some(Type::Error);
    }
    let t = ck.expr(&args[0], None);
    if t.is_error() {
        return Some(Type::Error);
    }
    if !t.is_fn() {
        ck.dg.error_note(
            args[0].span,
            format!("'{}' expects a function value, found {}", CODE_OF, ck.tcx.name_of(&t)),
            "the form is __code_of(f: fn(..) -> R) -> u64",
        );
        return Some(Type::Error);
    }
    Some(Type::U64)
}

/// Hook from `lower::lower_call`: word 0 of the record.
pub(crate) fn lower_call(lo: &mut Lower, args: &[Expr], span: Span) -> Option<Option<Val>> {
    if args.len() != 1 {
        return lo.ice(span, "__code_of with wrong arity");
    }
    let rec = lo.lower_expr(&args[0])?;
    Some(Some(lo.load(FTy::U64, rec)))
}
