// SPDX-License-Identifier: MPL-2.0
//! Lowering of the optional tracing GC to FIR (module `gckern`, SPEC §3.5).
//!
//! The language surface from `gc.rs` is mapped here onto three things:
//!
//!  * `gc C{ … }` — call of the runtime (`__gc_alloc_raw`), after that build
//!    the error union `AllocError!Gc[C]` and write the fields. The
//!    collection run sits inside `__gc_alloc_raw`: **collect first, then
//!    `AllocError::OutOfMemory`** (DESIGN_GOALS §2).
//!  * `weak(g)`, `strong(w)`, `x.as?[C]` — calls of the runtime.
//!  * `__gc_state()` / `__gc_save_regs()` — the two compiler intrinsics.
//!    `Op::GcAddr` yields the address of the state block; with `regs = true`
//!    the callee-saved registers get rescued there beforehand, so that the
//!    CONSERVATIVE register scan (SPEC §3.5.3) sees them.
//!
//! The **insertion barrier** sits in exactly one place: the write of a
//! `Gc[T]` pointer into a heap field (`hook_assign`). Since round B4 its fast
//! half is written INLINE (`emit_barrier`): count the write
//! (`gc_barriers()`), test the phase of the collector, and only while a
//! cycle runs call the runtime (`__gc_barrier_slow`), which greys the target.

use crate::ast::{Expr, ExprKind};
use crate::fir::{BinOp, CmpOp, FTy, Op, Val};
use crate::gc;
use crate::lower::Lower;
use crate::types::Type;

/// Call symbol that the runtime really carries. `weak`/`strong` are no
/// functions of the source text; they are mapped onto the runtime here.
pub(crate) fn real_name(name: &str) -> Option<&'static str> {
    match name {
        "weak" => Some(gc::FN_WEAK),
        "strong" => Some(gc::FN_STRONG),
        _ => None,
    }
}

/// `// HOOK gc` in `lower::lower_call`: allocation, intrinsics, `as?`.
/// Yields `Some(...)` once the call has been fully handled here.
pub(crate) fn hook_call(
    lo: &mut Lower,
    name: &str,
    args: &[Expr],
    dest: Option<Val>,
    span: crate::diag::Span,
) -> Option<Option<Option<Val>>> {
    if let Some(class) = gc::class_out_new(name) {
        let fields: Vec<(String, Expr, crate::diag::Span)> = match args.first().map(|a| &a.kind) {
            Some(ExprKind::StructLit(_, f, _)) => f.clone(),
            _ => Vec::new(),
        };
        let d = match dest {
            Some(d) => d,
            None => return Some(lo.ice(span, "gc allocation without target")),
        };
        let class = class.to_string();
        return Some(match alloc_and_init(lo, d, &class, &fields, span) {
            Some(()) => Some(None),
            None => None,
        });
    }
    // Round B4: an explicit `__gc_barrier(field, value)` -- the collections
    // in `lib/gc/gcvec.fi` / `gcmap.fi` write it by hand -- gets the same
    // inline fast path as the barrier the compiler inserts itself. The
    // field is still evaluated (order of side effects), not used.
    if name == gc::FN_BARRIER && args.len() == 2 {
        return Some((|| {
            lo.lower_expr(&args[0])?;
            let v = lo.lower_expr(&args[1])?;
            emit_barrier(lo, v);
            Some(None)
        })());
    }
    if name == gc::INTR_STATE || name == gc::INTR_REGS {
        let regs = name == gc::INTR_REGS;
        return Some(Some(Some(lo.push(FTy::Ptr, Op::GcAddr { regs }))));
    }
    let class = gc::class_out_as(name)?;
    let (tid, _, _) = gc::class_info(class)?;
    let arg = args.first()?;
    Some((|| {
        let p = lo.lower_expr(arg)?;
        let t = lo.constant(FTy::U64, tid as i128);
        Some(Some(lo.push(
            FTy::Ptr,
            Op::Call { name: gc::FN_AS.to_string(), args: vec![p, t] },
        )))
    })())
}

fn alloc_and_init(
    lo: &mut Lower,
    dest: Val,
    class: &str,
    fields: &[(String, Expr, crate::diag::Span)],
    span: crate::diag::Span,
) -> Option<()> {
    let (tid, size, sidx) = match gc::class_info(class) {
        Some(x) => x,
        None => return lo.ice(span, "gc allocation without class"),
    };
    // Position of `__err`/`__val` inside the error union.
    let union = match gc::union_idx(class).and_then(crate::errors::union_by_struct) {
        Some(u) => u,
        None => return lo.ice(span, "gc allocation without error union"),
    };
    let code = match crate::errors::variant_code(gc::ERR_SET, "OutOfMemory") {
        Some(c) => c,
        None => return lo.ice(span, "AllocError::OutOfMemory is missing"),
    };

    let tidv = lo.constant(FTy::U64, tid as i128);
    let sizev = lo.constant(FTy::U64, size as i128);
    let p = lo.push(
        FTy::Ptr,
        Op::Call { name: gc::FN_ALLOC.to_string(), args: vec![tidv, sizev] },
    );
    let null = lo.constant(FTy::Ptr, 0);
    let ok = lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Ne, ty: FTy::Ptr, a: p, b: null });
    let ok_bb = lo.new_block();
    let fail_bb = lo.new_block();
    let join = lo.new_block();
    lo.set_term(crate::fir::Term::BrCond { cond: ok, then_bb: ok_bb, else_bb: fail_bb });

    // Error case: the runtime collected first, after that it really is over.
    lo.cur = fail_bb;
    let c = lo.constant(FTy::U32, code);
    lo.store(FTy::U32, dest, c);
    let va = lo.field_addr_at(dest, union.val_off);
    let z = lo.constant(FTy::Ptr, 0);
    lo.store(FTy::Ptr, va, z);
    lo.set_term(crate::fir::Term::Br(join));

    // Success case: fill the error union (so that the pointer immediately has
    // a root on the stack), then write the fields.
    lo.cur = ok_bb;
    let zero = lo.constant(FTy::U32, 0);
    lo.store(FTy::U32, dest, zero);
    let va = lo.field_addr_at(dest, union.val_off);
    lo.store(FTy::Ptr, va, p);
    let decl: Vec<(String, u64)> = match lo.info.tcx.structs.get(sidx) {
        Some(d) => d.fields.iter().map(|f| (f.name.clone(), f.offset)).collect(),
        None => Vec::new(),
    };
    for (fname, fexpr, fspan) in fields {
        let off = match decl.iter().find(|(n, _)| n == fname) {
            Some((_, o)) => *o,
            None => return lo.ice(*fspan, "unknown field in the gc allocation"),
        };
        let fa = lo.ptradd_const(p, off);
        lo.write_into(fa, fexpr)?;
    }
    if !lo.terminated() {
        lo.set_term(crate::fir::Term::Br(join));
    }
    lo.cur = join;
    Some(())
}

/// `// HOOK gc` in `lower::lower_stmt` (assignment): the insertion barrier.
/// It is called AFTER the write; `target` is the field written to and
/// `addr` the address the store went to (computed once, by the caller).
pub(crate) fn hook_assign(lo: &mut Lower, target: &Expr, addr: Val) -> Option<()> {
    let t = lo.ty_of(target);
    if !gc::is_gc_ptr(&t) {
        return Some(());
    }
    // Only writes INTO the heap need the barrier; a local variable
    // lives on the stack.
    let ins_heap = match &target.kind {
        ExprKind::Field(base, _, _) => gc::is_gc_ptr(&lo.ty_of(base)) || is_ptr(&lo.ty_of(base)),
        ExprKind::Index(base, _) => is_ptr(&lo.ty_of(base)),
        ExprKind::Unary(crate::ast::UnOp::Deref, _) => true,
        _ => false,
    };
    if !ins_heap {
        return Some(());
    }
    // Round B4: the address the store went to, NOT a second evaluation of
    // the target -- `pick().next = x` used to call `pick()` twice
    // (tests/1664_gc_barrier_inline.fi counts the calls).
    let val = lo.load(FTy::Ptr, addr);
    emit_barrier(lo, val);
    Some(())
}

/// **Round B4** -- the insertion barrier, written INLINE at the store.
///
/// Before, every `Gc` pointer store was a call of `__gc_barrier` (measured
/// 11.7 ns per store at dev-fast; the inliner only removed it on
/// release-*). The runtime function did two things on the fast path --
/// count the store, look at the phase -- and that is exactly what is
/// emitted here:
///
/// ```text
///     st  = gc_addr                      ; lea of the state block
///     [st + S_BARRIEREN] += 1            ; gc_barriers() stays exact
///     if [st + S_PHASE] != 0 { __gc_barrier_slow(value) }
/// ```
///
/// The slow half (greying, the finalizer check, the lock) stays in the
/// runtime and runs only while a cycle is marking or sweeping. The contract
/// between compiler and runtime is therefore two offsets and one name
/// (`gc.rs`, checked by `barrier_offsets_match_the_runtime`).
///
/// The old runtime test `field == 0 && value == 0` is gone on purpose: the
/// field address of a store that just happened is never 0.
pub(crate) fn emit_barrier(lo: &mut Lower, value: Val) {
    let st = lo.push(FTy::Ptr, Op::GcAddr { regs: false });
    let ca = lo.ptradd_const(st, gc::BARRIER_COUNT_OFF);
    let c = lo.load(FTy::U64, ca);
    let one = lo.constant(FTy::U64, 1);
    let c1 = lo.push(FTy::U64, Op::Bin(BinOp::Add, c, one));
    lo.store(FTy::U64, ca, c1);
    let pa = lo.ptradd_const(st, gc::PHASE_OFF);
    let ph = lo.load(FTy::U64, pa);
    let zero = lo.constant(FTy::U64, 0);
    let busy = lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Ne, ty: FTy::U64, a: ph, b: zero });
    let slow = lo.new_block();
    let join = lo.new_block();
    lo.set_term(crate::fir::Term::BrCond { cond: busy, then_bb: slow, else_bb: join });
    lo.cur = slow;
    lo.push_void(
        FTy::Void,
        Op::Call { name: gc::FN_BARRIER_SLOW.to_string(), args: vec![value] },
    );
    lo.set_term(crate::fir::Term::Br(join));
    lo.cur = join;
}

fn is_ptr(t: &Type) -> bool {
    matches!(t, Type::Ptr { .. })
}
