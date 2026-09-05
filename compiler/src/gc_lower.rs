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
use crate::fir::{BinOp, CmpOp, FTy, Op, Term, Val};
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


/// **ROUND ANLEGEWEG** -- the fast path of the allocation, AT the call site.
///
/// Up to here every `gc C{ }` was one call of `__gc_alloc_raw`, and that cost
/// 144 ns -- with the collector switched off exactly the same, so it was
/// never the collecting. It was the way there: some three dozen calls inside
/// the runtime, and at the head of them the question which size class the
/// object belongs to and how big its block is. **That answer is a constant at
/// the call site** and is folded in here.
///
/// What is emitted:
///
/// ```text
///   st = &state
///   if [st+INIT]  != 1     -> slow
///   if [st+MULTI] != 0     -> slow      several threads: lock and safepoint
///   if [st+PHASE] != 0     -> slow      a cycle runs: the object must be grey
///   if [st+SINCE] >= [st+LIMIT] -> slow the next run is due
///   b = [st + FREE + class*8]
///   if b == 0              -> slow      the list of this class is empty
///   [st + FREE + class*8] = [b+16]      unhook
///   [b+16 .. b+16+size]   = 0           null the payload (unrolled)
///   ser = [st+SERIES] + 1 ; [st+SERIES] = ser
///   [b]   = tid  (u32)                  the type id, a constant
///   [b+4] = [st+PAR] (u32)              the current white
///   [b+8] = ser
///   [st+SINCE] += step ; [st+TOTAL] += step ; [st+D_ALLOK] += 1
///   p = b + 16
/// slow:
///   p = call __gc_alloc_raw(tid, size)
/// ```
///
/// Every one of the five conditions is the same one the long path asks; not
/// one of them is skipped. The arithmetic is `+%` (wrapping) throughout: these
/// are ADDRESSES inside the state block and inside a chunk, an overflow there
/// is impossible by construction, and a check on every one of them was
/// exactly what made the runtime slow. The counters are the same ones and are
/// kept in the same way -- `gc_total_bytes()`, `gc_diag(6)` and the serial
/// numbers of `GcWeak` keep their meaning down to the last octet.
///
/// The result travels through the `__val` cell OF THE ERROR UNION -- not
/// through a stack cell of its own. FIR has no phi, so a cell there has to
/// be; but it must not be a NEW one.
///
/// **That is not a matter of taste, it is the round's own bug report.** A
/// cell of its own held the last pointer allocated at this site FOR AS LONG
/// AS THE FUNCTION RAN, and nothing ever cleared it. The stack scan is
/// CONSERVATIVE: such a cell is a root. `tests/842_gcmap_basic.fi` and
/// `tests/843_collections_interplay.fi` -- the two that prove an object is
/// NOT held any more -- went red by exactly one object. The `__val` cell of
/// the error union is where the pointer belongs anyway, it already is a
/// root, and the failure branch overwrites it with the null value right
/// after.
fn alloc_fast(lo: &mut Lower, tid: u64, size: u64, slot: Val) -> Option<Val> {
    let class = gc::class_for(size);
    let slow_bb = lo.new_block();
    let done_bb = lo.new_block();
    if class < gc::CLASS_BYTES.len() {
        let step = gc::BLOCK_HEADER + gc::CLASS_BYTES[class];
        let free_off = gc::ST_FREE + class as u64 * 8;
        let st = lo.push(FTy::Ptr, Op::GcAddr { regs: false });

        // the four questions about the state, each one branch into the slow path
        let mut guard = |lo: &mut Lower, off: u64, op: CmpOp, rhs: Val| {
            let a = lo.ptradd_const(st, off);
            let v = lo.load(FTy::U64, a);
            let c = lo.push(FTy::Bool, Op::Cmp { op, ty: FTy::U64, a: v, b: rhs });
            let go = lo.new_block();
            lo.set_term(Term::BrCond { cond: c, then_bb: go, else_bb: slow_bb });
            lo.cur = go;
            v
        };
        let one = lo.constant(FTy::U64, 1);
        guard(lo, gc::ST_INIT, CmpOp::Eq, one);
        let zero = lo.constant(FTy::U64, 0);
        guard(lo, gc::ST_MULTI, CmpOp::Eq, zero);
        guard(lo, gc::ST_PHASE, CmpOp::Eq, zero);
        let limit_a = lo.ptradd_const(st, gc::ST_LIMIT);
        let limit = lo.load(FTy::U64, limit_a);
        let since = guard(lo, gc::ST_SINCE, CmpOp::Lt, limit);

        // the head of the free list of this class
        let head_a = lo.ptradd_const(st, free_off);
        let b = lo.load(FTy::Ptr, head_a);
        let null = lo.constant(FTy::Ptr, 0);
        let has = lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Ne, ty: FTy::Ptr, a: b, b: null });
        let take_bb = lo.new_block();
        lo.set_term(Term::BrCond { cond: has, then_bb: take_bb, else_bb: slow_bb });
        lo.cur = take_bb;

        // unhook: the link lies in the first word of the payload
        let obj = lo.ptradd_const(b, gc::BLOCK_HEADER);
        let next = lo.load(FTy::Ptr, obj);
        lo.store(FTy::Ptr, head_a, next);

        // null the payload -- unrolled, `size` is a constant here
        let z64 = lo.constant(FTy::U64, 0);
        let mut off = 0u64;
        while off < size {
            let a = lo.ptradd_const(obj, off);
            lo.store(FTy::U64, a, z64);
            off += 8;
        }

        // serial number
        let ser_a = lo.ptradd_const(st, gc::ST_SERIES);
        let ser0 = lo.load(FTy::U64, ser_a);
        let ser = lo.push(FTy::U64, Op::Bin(BinOp::Add, ser0, one));
        lo.store(FTy::U64, ser_a, ser);

        // the header: type id, the current white, serial number
        let tidv = lo.constant(FTy::U32, tid as i128);
        lo.store(FTy::U32, b, tidv);
        let par_a = lo.ptradd_const(st, gc::ST_PAR);
        let par = lo.load(FTy::U32, par_a);
        let mark_a = lo.ptradd_const(b, 4);
        lo.store(FTy::U32, mark_a, par);
        let ser_at = lo.ptradd_const(b, 8);
        lo.store(FTy::U64, ser_at, ser);

        // the three counters
        let stepv = lo.constant(FTy::U64, step as i128);
        let since_a = lo.ptradd_const(st, gc::ST_SINCE);
        let since_n = lo.push(FTy::U64, Op::Bin(BinOp::Add, since, stepv));
        lo.store(FTy::U64, since_a, since_n);
        let tot_a = lo.ptradd_const(st, gc::ST_TOTAL);
        let tot = lo.load(FTy::U64, tot_a);
        let tot_n = lo.push(FTy::U64, Op::Bin(BinOp::Add, tot, stepv));
        lo.store(FTy::U64, tot_a, tot_n);
        let al_a = lo.ptradd_const(st, gc::ST_D_ALLOK);
        let al = lo.load(FTy::U64, al_a);
        let al_n = lo.push(FTy::U64, Op::Bin(BinOp::Add, al, one));
        lo.store(FTy::U64, al_a, al_n);

        lo.store(FTy::Ptr, slot, obj);
        lo.set_term(Term::Br(done_bb));
    } else {
        // no size class of its own -- straight into the runtime
        lo.set_term(Term::Br(slow_bb));
    }

    // the slow path: everything the runtime has always done
    lo.cur = slow_bb;
    let tidv = lo.constant(FTy::U64, tid as i128);
    let sizev = lo.constant(FTy::U64, size as i128);
    let p = lo.push(
        FTy::Ptr,
        Op::Call { name: gc::FN_ALLOC.to_string(), args: vec![tidv, sizev] },
    );
    lo.store(FTy::Ptr, slot, p);
    lo.set_term(Term::Br(done_bb));

    lo.cur = done_bb;
    Some(lo.load(FTy::Ptr, slot))
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

    // ROUND ANLEGEWEG: the fast path stands here, folded to constants; the
    // runtime is only reached when the ordinary case does not hold.
    let vaddr = lo.field_addr_at(dest, union.val_off);
    let p = match alloc_fast(lo, tid, size, vaddr) {
        Some(p) => p,
        None => return lo.ice(span, "gc allocation without a fast path"),
    };
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
