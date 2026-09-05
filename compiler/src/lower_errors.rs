// SPDX-License-Identifier: GPL-2.0-only
//! Lowering of the error unions `E!T` to FIR (module `fehlerunionen`, SPEC §5.1).
//!
//! An error union is a plain struct (`errors.rs`):
//! `{ __err: u32, __val: T }`, `__err == 0` means success. So no new FIR
//! instruction is needed here — just `load`/`store`, one comparison and one
//! branch.
//!
//! What is produced:
//!  * `IoError::NotFound` — `store __err = code`
//!  * implicit conversion at `return`/`let`/assignment — `__err = 0` plus the
//!    value, respectively `__err = code`
//!  * `try a` — `if a.__err != 0 { return error(a.__err) }`, else `a.__val`
//!  * `a catch b` — `if a.__err == 0 { a.__val } else { b }`

use std::cell::RefCell;
use std::collections::HashSet;

use crate::ast::{Expr, ExprId, ExprKind};
use crate::errors::{
    catch_bind, catch_of, coerce_of, error_set_of, is_error_set_name, try_of, variant_code,
    CoerceKind, CATCH_NAME, TRY_NAME,
};
use crate::fir::{CmpOp, FTy, Op, Term, Val};
use crate::lower::Lower;
use crate::types::Type;

/// Which sort of error union expression is this?
enum Kind {
    Try,
    Catch,
    /// `ErrorSet::Variant`
    Ctor(i128),
}

thread_local! {
    /// Expressions whose implicit conversion is being produced right now. Keeps
    /// the access to the raw value from running into the conversion again.
    static BUSY: RefCell<HashSet<ExprId>> = RefCell::new(HashSet::new());
}

fn busy(id: ExprId) -> bool {
    BUSY.with(|b| b.borrow().contains(&id))
}

fn pending_coerce(e: &Expr) -> Option<crate::errors::CoerceInfo> {
    if busy(e.id) {
        return None;
    }
    coerce_of(e.id)
}

fn kind_of(e: &Expr) -> Option<Kind> {
    let name = match &e.kind {
        ExprKind::Call(n, _, _) => n.as_str(),
        _ => return None,
    };
    if name == TRY_NAME {
        return Some(Kind::Try);
    }
    if name == CATCH_NAME {
        return Some(Kind::Catch);
    }
    let (set, variant) = name.split_once("::")?;
    if !is_error_set_name(set) {
        return None;
    }
    Some(Kind::Ctor(variant_code(set, variant)?))
}

fn args_of(e: &Expr) -> &[Expr] {
    match &e.kind {
        ExprKind::Call(_, a, _) => a,
        _ => &[],
    }
}

fn is_agg(t: &Type) -> bool {
    matches!(t, Type::Array(..) | Type::Struct(_))
}

// -------------------------------------------------------------------- Hooks

/// `// HOOK fehlerunionen` in `lower::lower_addr_inner`.
pub(crate) fn hook_addr(lo: &mut Lower, e: &Expr) -> Option<Option<Val>> {
    if let Some(c) = pending_coerce(e) {
        let (size, align) = (c.union.size.max(1), c.union.align.max(1));
        let slot = lo.alloca(size, align);
        return Some(match write_union(lo, slot, e, &c) {
            Some(()) => Some(slot),
            None => None,
        });
    }
    let k = kind_of(e)?;
    Some(match k {
        Kind::Try => try_value_addr(lo, e),
        Kind::Catch => catch_slot(lo, e),
        Kind::Ctor(code) => ctor_addr(lo, e, code),
    })
}

/// `// HOOK fehlerunionen` in `lower::write_into_inner`.
pub(crate) fn hook_write_into(lo: &mut Lower, addr: Val, e: &Expr) -> Option<Option<()>> {
    if let Some(c) = pending_coerce(e) {
        return Some(write_union(lo, addr, e, &c));
    }
    let k = kind_of(e)?;
    Some(match k {
        Kind::Ctor(code) => {
            let c = lo.constant(FTy::U32, code);
            lo.store(FTy::U32, addr, c);
            Some(())
        }
        Kind::Try | Kind::Catch => {
            let src = match k {
                Kind::Try => try_value_addr(lo, e),
                _ => catch_slot(lo, e),
            }?;
            let t = lo.ty_of(e);
            copy_value(lo, addr, src, &t)
        }
    })
}

/// `// HOOK fehlerunionen` in `lower::lower_expr_inner` (scalar result).
pub(crate) fn hook_value(lo: &mut Lower, e: &Expr) -> Option<Option<Val>> {
    let k = kind_of(e)?;
    let t = lo.ty_of(e);
    if is_agg(&t) {
        return None; // aggregates run through `hook_addr`
    }
    let addr = match k {
        Kind::Try => try_value_addr(lo, e),
        Kind::Catch => catch_slot(lo, e),
        Kind::Ctor(_) => return None,
    };
    Some(match addr {
        Some(a) => {
            let ft = lo.fty_of(e)?;
            Some(lo.load(ft, a))
        }
        None => None,
    })
}

/// `// HOOK fehlerunionen` in `lower::lower_expr_stmt`.
pub(crate) fn hook_stmt(lo: &mut Lower, e: &Expr) -> Option<Option<()>> {
    let k = kind_of(e)?;
    Some(match k {
        Kind::Try => try_value_addr(lo, e).map(|_| ()),
        Kind::Catch => catch_slot(lo, e).map(|_| ()),
        Kind::Ctor(code) => {
            let (size, align) = lo.size_align(&lo.ty_of(e));
            let slot = lo.alloca(size, align);
            let c = lo.constant(FTy::U32, code);
            lo.store(FTy::U32, slot, c);
            Some(())
        }
    })
}

/// `// HOOK fehlerunionen` in `lower::lower_stmt` (`return value`).
/// Yields `None` when no implicit conversion is needed.
pub(crate) fn hook_return(lo: &mut Lower, v: &Expr) -> Option<Option<()>> {
    let c = pending_coerce(v)?;
    Some(do_return(lo, v, &c))
}

/// `// HOOK fehlerunionen` in `lower::lower_stmt` (`let x: E!T = value`).
pub(crate) fn hook_let(lo: &mut Lower, name: &str, init: &Expr) -> Option<Option<()>> {
    let c = pending_coerce(init)?;
    let (size, align) = (c.union.size.max(1), c.union.align.max(1));
    let slot = lo.alloca(size, align);
    Some(match write_union(lo, slot, init, &c) {
        Some(()) => {
            lo.declare(name, slot);
            Some(())
        }
        None => None,
    })
}

/// `// HOOK fehlerunionen` in `lower::lower_call`: the type of an argument
/// converted implicitly into an error union — that argument crosses the
/// call boundary as an aggregate (abi.rs), not as a success value.
pub(crate) fn hook_arg_type(e: &Expr) -> Option<Type> {
    let c = pending_coerce(e)?;
    Some(Type::Struct(c.union.struct_idx))
}

/// `// HOOK fehlerunionen` in `lower::lower_binary`: `e == E::NotFound`.
pub(crate) fn hook_binary(
    lo: &mut Lower,
    op: crate::ast::BinOp,
    a: &Expr,
    b: &Expr,
) -> Option<Option<Val>> {
    use crate::ast::BinOp;
    let cmp = match op {
        BinOp::Eq => CmpOp::Eq,
        BinOp::Ne => CmpOp::Ne,
        _ => return None,
    };
    error_set_of(&lo.ty_of(a))?;
    error_set_of(&lo.ty_of(b))?;
    Some(cmp_codes(lo, cmp, a, b))
}

fn cmp_codes(lo: &mut Lower, cmp: CmpOp, a: &Expr, b: &Expr) -> Option<Val> {
    let aa = lo.lower_addr(a)?;
    let av = lo.load(FTy::U32, aa);
    let ba = lo.lower_addr(b)?;
    let bv = lo.load(FTy::U32, ba);
    Some(lo.push(
        FTy::Bool,
        Op::Cmp { op: cmp, ty: FTy::U32, a: av, b: bv },
    ))
}

// ---------------------------------------------------------- Building blocks

/// Writes the error union to `addr` — success value or error code.
fn write_union(lo: &mut Lower, addr: Val, e: &Expr, c: &crate::errors::CoerceInfo) -> Option<()> {
    BUSY.with(|b| b.borrow_mut().insert(e.id));
    let r = write_union_inner(lo, addr, e, c);
    BUSY.with(|b| {
        b.borrow_mut().remove(&e.id);
    });
    r
}

fn write_union_inner(
    lo: &mut Lower,
    addr: Val,
    e: &Expr,
    c: &crate::errors::CoerceInfo,
) -> Option<()> {
    match c.kind {
        CoerceKind::FromValue => {
            let zero = lo.constant(FTy::U32, 0);
            lo.store(FTy::U32, addr, zero);
            // Layer field access <-> storage location (layout.rs, DESIGN_GOALS 8)
            let va = lo.field_addr_at(addr, c.union.val_off);
            lo.write_into(va, e)
        }
        CoerceKind::FromError => {
            let code = error_code(lo, e)?;
            lo.store(FTy::U32, addr, code);
            Some(())
        }
    }
}

/// Error code of an expression with the type of an error set.
fn error_code(lo: &mut Lower, e: &Expr) -> Option<Val> {
    if let Some(Kind::Ctor(code)) = kind_of(e) {
        return Some(lo.constant(FTy::U32, code));
    }
    let a = lo.lower_addr(e)?;
    Some(lo.load(FTy::U32, a))
}

fn ctor_addr(lo: &mut Lower, e: &Expr, code: i128) -> Option<Val> {
    let (size, align) = lo.size_align(&lo.ty_of(e));
    let slot = lo.alloca(size, align);
    let c = lo.constant(FTy::U32, code);
    lo.store(FTy::U32, slot, c);
    Some(slot)
}

/// Copies a value of type `t` from `src` to `dst`.
fn copy_value(lo: &mut Lower, dst: Val, src: Val, t: &Type) -> Option<()> {
    if is_agg(t) {
        let size = lo.size_align(t).0;
        lo.push_void(FTy::Void, Op::CopyMem { dst, src, size });
        return Some(());
    }
    let ft = match scalar_fty(t) {
        Some(f) => f,
        None => return Some(()),
    };
    let v = lo.load(ft, src);
    lo.store(ft, dst, v);
    Some(())
}

/// **ROUND 68** — this used to be a COPY of `lower::scalar_fty`, and the copy
/// had lost `Type::F64`. `copy_value` therefore silently copied NOTHING for
/// an `E!f64`: the success value never reached its slot, and what came out
/// the other side was whatever happened to lie there
/// (docs/ROUND63.md, gap 2 — "this bug is dangerous because it is SILENT").
/// There is only ONE table now; a type added to the language cannot be
/// forgotten here a second time.
fn scalar_fty(t: &Type) -> Option<FTy> {
    match crate::lower::scalar_fty_pub(t) {
        // `void` carries no value that could be copied. The `None` says
        // "nothing to do", not "unknown type" — that difference is why this
        // wrapper exists at all.
        None | Some(FTy::Void) => None,
        Some(f) => Some(f),
    }
}

/// `return` with implicit conversion into the error union of the function.
fn do_return(lo: &mut Lower, v: &Expr, c: &crate::errors::CoerceInfo) -> Option<()> {
    // `return E::Variant` is the error path, `return value` the success path —
    // the type checker has decided that already (`CoerceKind`).
    let error_path = matches!(c.kind, CoerceKind::FromError);
    match lo.sret {
        Some(dst) => {
            write_union(lo, dst, v, c)?;
            if error_path {
                lo.ret_term_error(Some(dst));
            } else {
                lo.ret_term(Some(dst));
            }
        }
        None => {
            // Up to 8 bytes the error union sits as one word in `rax`
            // (abi.rs). The scratch slot is zeroed, so that the
            // padding bytes hold the same value at every build stage.
            let slot = lo.alloca(8, 8);
            let zero = lo.constant(FTy::I64, 0);
            lo.store(FTy::I64, slot, zero);
            write_union(lo, slot, v, c)?;
            let w = lo.load(FTy::I64, slot);
            if error_path {
                lo.ret_term_error(Some(w));
            } else {
                lo.ret_term(Some(w));
            }
        }
    }
    Some(())
}

/// Leaves the function carrying the error code `code` (`try`).
fn return_error(lo: &mut Lower, code: Val) -> Option<()> {
    // ERROR PATH: the `errdefer` statements run here as well.
    match lo.sret {
        Some(dst) => {
            lo.store(FTy::U32, dst, code);
            lo.ret_term_error(Some(dst));
        }
        None => {
            let slot = lo.alloca(8, 8);
            let zero = lo.constant(FTy::I64, 0);
            lo.store(FTy::I64, slot, zero);
            lo.store(FTy::U32, slot, code);
            let w = lo.load(FTy::I64, slot);
            lo.ret_term_error(Some(w));
        }
    }
    Some(())
}

/// `try a` — yields the address of the success value; on the error case the
/// function is left carrying the same code.
fn try_value_addr(lo: &mut Lower, e: &Expr) -> Option<Val> {
    let ti = match try_of(e.id) {
        Some(t) => t,
        None => return lo.ice(e.span, "'try' without error union"),
    };
    let arg = match args_of(e).first() {
        Some(a) => a.clone(),
        None => return lo.ice(e.span, "'try' without operand"),
    };
    let src = lo.lower_addr(&arg)?;
    let code = lo.load(FTy::U32, src);
    let zero = lo.constant(FTy::U32, 0);
    let bad = lo.push(
        FTy::Bool,
        Op::Cmp { op: CmpOp::Ne, ty: FTy::U32, a: code, b: zero },
    );
    let fail = lo.new_block();
    let cont = lo.new_block();
    lo.set_term(Term::BrCond { cond: bad, then_bb: fail, else_bb: cont });
    lo.cur = fail;
    return_error(lo, code)?;
    lo.cur = cont;
    // Layer field access <-> storage location (layout.rs, DESIGN_GOALS 8)
    Some(lo.field_addr_at(src, ti.inner.val_off))
}

/// `a catch b` — yields the address of the result (success value or fallback).
fn catch_slot(lo: &mut Lower, e: &Expr) -> Option<Val> {
    let ci = match catch_of(e.id) {
        Some(c) => c,
        None => return lo.ice(e.span, "'catch' without error union"),
    };
    let args = args_of(e).to_vec();
    let (lhs, rhs) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return lo.ice(e.span, "'catch' without two operands"),
    };
    let (size, align) = lo.size_align(&ci.inner.val_ty);
    let slot = lo.alloca(size, align);
    let src = lo.lower_addr(&lhs)?;
    let code = lo.load(FTy::U32, src);
    let zero = lo.constant(FTy::U32, 0);
    let good = lo.push(
        FTy::Bool,
        Op::Cmp { op: CmpOp::Eq, ty: FTy::U32, a: code, b: zero },
    );
    let ok_bb = lo.new_block();
    let old_bb = lo.new_block();
    let join = lo.new_block();
    lo.set_term(Term::BrCond { cond: good, then_bb: ok_bb, else_bb: old_bb });

    lo.cur = ok_bb;
    // Layer field access <-> storage location (layout.rs, DESIGN_GOALS 8)
    let va = lo.field_addr_at(src, ci.inner.val_off);
    copy_value(lo, slot, va, &ci.inner.val_ty)?;
    if !lo.terminated() {
        lo.set_term(Term::Br(join));
    }

    lo.cur = old_bb;
    let bind = catch_bind(e.id);
    if let Some(name) = &bind {
        // `catch |e| …`: the error value appears in the fallback expression
        lo.enter();
        let eslot = lo.alloca(4, 4);
        lo.store(FTy::U32, eslot, code);
        lo.declare(name, eslot);
    }
    lo.write_into(slot, &rhs)?;
    if bind.is_some() {
        lo.leave();
    }
    if !lo.terminated() {
        lo.set_term(Term::Br(join));
    }

    lo.cur = join;
    Some(slot)
}
