// SPDX-License-Identifier: GPL-2.0-only
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
//! `Gc[T]` pointer into a heap field (`hook_assign`). At this stage it counts
//! the writes (`gc_barriers()`); the collector stops the world, so mark-sweep
//! needs no greying here. The slot for incremental collection (`S5`, still
//! open) is thereby present and provably exercised.

use crate::ast::{Expr, ExprKind};
use crate::fir::{CmpOp, FTy, Op, Val};
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
/// It is called AFTER the write; `target` is the field written to.
pub(crate) fn hook_assign(lo: &mut Lower, target: &Expr) -> Option<()> {
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
    let addr = lo.lower_addr(target)?;
    let val = lo.load(FTy::Ptr, addr);
    lo.push_void(
        FTy::Void,
        Op::Call { name: gc::FN_BARRIER.to_string(), args: vec![addr, val] },
    );
    Some(())
}

fn is_ptr(t: &Type) -> bool {
    matches!(t, Type::Ptr { .. })
}
