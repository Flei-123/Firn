//! Lowering des Opt-in-Tracing-GC nach FIR (Modul `gckern`, SPEC §3.5).
//!
//! Die Sprachoberflaeche aus `gc.rs` wird hier auf drei Dinge abgebildet:
//!
//!  * `gc C{ … }` — Aufruf der Laufzeit (`__gc_alloc_raw`), danach die
//!    Fehlerunion `AllocError!Gc[C]` bauen und die Felder schreiben. Der
//!    Sammellauf steckt in `__gc_alloc_raw`: **erst sammeln, dann
//!    `AllocError::OutOfMemory`** (DESIGNZIELE §2).
//!  * `weak(g)`, `stark(w)`, `x.as?[C]` — Aufrufe der Laufzeit.
//!  * `__gc_state()` / `__gc_save_regs()` — die beiden Compilerintrinsics.
//!    `Op::GcAddr` liefert die Adresse des Zustandsblocks; mit `regs = true`
//!    werden vorher die callee-saved Register dorthin gerettet, damit der
//!    KONSERVATIVE Registerscan (SPEC §3.5.3) sie sieht.
//!
//! Die **Einfuegebarriere** sitzt an genau einer Stelle: dem Schreiben eines
//! `Gc[T]`-Zeigers in ein Heapfeld (`hook_assign`). In dieser Stufe zaehlt sie
//! die Schreibzugriffe (`gc_barriers()`); der Sammler haelt an, also braucht
//! Mark-Sweep hier keine Graufaerbung. Der Platz fuer das inkrementelle
//! Sammeln (`S5`, offen) ist damit vorhanden und nachweisbar durchlaufen.

use crate::ast::{Expr, ExprKind};
use crate::fir::{CmpOp, FTy, Op, Val};
use crate::gc;
use crate::lower::Lower;
use crate::types::Type;

/// Aufrufname, den die Laufzeit wirklich traegt. `weak`/`stark` sind keine
/// Funktionen des Quelltextes; sie werden hier auf die Laufzeit abgebildet.
pub(crate) fn real_name(name: &str) -> Option<&'static str> {
    match name {
        "weak" => Some(gc::FN_WEAK),
        "strong" => Some(gc::FN_STARK),
        _ => None,
    }
}

/// `// HOOK gc` in `lower::lower_call`: Allokation, Intrinsics, `as?`.
/// Liefert `Some(...)`, wenn der Aufruf hier vollstaendig erledigt wurde.
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
            None => return Some(lo.ice(span, "gc-allokation ohne ziel")),
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
        None => return lo.ice(span, "gc-allokation ohne klasse"),
    };
    // Lage von `__err`/`__val` in der Fehlerunion.
    let union = match gc::union_idx(class).and_then(crate::errors::union_by_struct) {
        Some(u) => u,
        None => return lo.ice(span, "gc-allokation ohne fehlerunion"),
    };
    let code = match crate::errors::variant_code(gc::ERR_SET, "OutOfMemory") {
        Some(c) => c,
        None => return lo.ice(span, "AllocError::OutOfMemory fehlt"),
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

    // Fehlerfall: erst hat die Laufzeit gesammelt, dann ist wirklich Schluss.
    lo.cur = fail_bb;
    let c = lo.constant(FTy::U32, code);
    lo.store(FTy::U32, dest, c);
    let va = lo.field_addr_at(dest, union.val_off);
    let z = lo.constant(FTy::Ptr, 0);
    lo.store(FTy::Ptr, va, z);
    lo.set_term(crate::fir::Term::Br(join));

    // Erfolgsfall: Fehlerunion fuellen (damit der Zeiger sofort eine Wurzel
    // auf dem Stapel hat), dann die Felder schreiben.
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
            None => return lo.ice(*fspan, "unbekanntes feld in der gc-allokation"),
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

/// `// HOOK gc` in `lower::lower_stmt` (Zuweisung): die Einfuegebarriere.
/// Wird NACH dem Schreiben gerufen; `target` ist das beschriebene Feld.
pub(crate) fn hook_assign(lo: &mut Lower, target: &Expr) -> Option<()> {
    let t = lo.ty_of(target);
    if !gc::is_gc_ptr(&t) {
        return Some(());
    }
    // Nur Schreibzugriffe IN den Heap brauchen die Barriere; eine oertliche
    // Veraenderliche liegt auf dem Stapel.
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
