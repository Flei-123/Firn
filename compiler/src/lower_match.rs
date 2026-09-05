// SPDX-License-Identifier: GPL-2.0-only
//! Lowering of `match` and enum constructors to FIR (module `types`).
//!
//! The result is a `fir::Term::Switch` over the variant number (or over the
//! integer value) whose cases are **sorted ascending and free of duplicates**.
//! Subpatterns (nested variants, literals, ranges) are checked as a chain of
//! comparisons BEHIND the switch; whenever a candidate does not fit, control
//! moves to the next candidate of the same key and finally to the catch-all
//! chain.

use std::collections::BTreeMap;

use crate::ast::Expr;
use crate::diag::Span;
use crate::fir::{BlockId, CmpOp, FTy, Op, Term, Val};
use crate::lower::Lower;
use crate::sema_match::{enum_by_struct, match_info, EnumDef, MatchInfo, Pattern, MATCH_PREFIX};
use crate::types::Type;

/// Range patterns up to this width are resolved into separate jump labels;
/// wider ones are checked as a comparison.
const MAX_RANGE_KEYS: i128 = 256;

pub(crate) fn is_ctor(name: &str) -> bool {
    name.contains("::")
}

pub(crate) fn is_types_call(name: &str) -> bool {
    name.starts_with(MATCH_PREFIX) || is_ctor(name)
}

fn scalar_fty(t: &Type) -> Option<FTy> {
    Some(match t {
        Type::I8 => FTy::I8,
        Type::I16 => FTy::I16,
        Type::I32 => FTy::I32,
        Type::I64 | Type::Isize => FTy::I64,
        Type::U8 => FTy::U8,
        Type::U16 => FTy::U16,
        Type::U32 => FTy::U32,
        Type::U64 | Type::Usize => FTy::U64,
        Type::Bool => FTy::Bool,
        Type::Ptr { .. } => FTy::Ptr,
        // Round 58: a function value is the pointer to its function record.
        Type::Fn { .. } => FTy::Ptr,
        _ => return None,
    })
}

// -------------------------------------------------------------- Constructors

/// `Enum::Variant(a, b)` — builds the value and yields its address.
pub(crate) fn lower_ctor_addr(
    lo: &mut Lower,
    e: &Expr,
    name: &str,
    args: &[Expr],
) -> Option<Val> {
    let t = lo.ty_of(e);
    let (size, align) = lo.size_align(&t);
    let slot = lo.alloca(size, align);
    write_ctor_into(lo, e, name, args, slot)?;
    Some(slot)
}

/// Writes `Enum::Variant(a, b)` straight to the address `addr`.
pub(crate) fn write_ctor_into(
    lo: &mut Lower,
    e: &Expr,
    name: &str,
    args: &[Expr],
    addr: Val,
) -> Option<()> {
    let t = lo.ty_of(e);
    let sidx = match &t {
        Type::Struct(i) => *i,
        _ => return lo.ice(e.span, "enum constructor without enum type"),
    };
    let def = match enum_by_struct(sidx) {
        Some(d) => d,
        None => return lo.ice(e.span, "enum constructor without enum"),
    };
    let vname = match name.split_once("::") {
        Some((_, v)) => v.to_string(),
        None => return lo.ice(e.span, "enum constructor without variant name"),
    };
    let v = match def.variant(&vname) {
        Some(v) => v.clone(),
        None => return lo.ice(e.span, "unknown variant in lowering"),
    };
    let tag = lo.constant(FTy::U32, v.tag);
    lo.store(FTy::U32, addr, tag);
    for (i, a) in args.iter().enumerate() {
        let off = match v.offsets.get(i) {
            Some(o) => *o,
            None => return lo.ice(a.span, "payload field without offset"),
        };
        // Layer field access <-> storage location (layout.rs, DESIGN_GOALS 8)
        let ad = lo.field_addr_at(addr, off);
        lo.write_into(ad, a)?;
    }
    Some(())
}

/// `// HOOK types` in `lower::lower_expr_stmt`.
pub(crate) fn lower_types_stmt(
    lo: &mut Lower,
    e: &Expr,
    name: &str,
    args: &[Expr],
) -> Option<()> {
    if let Some(idx) = name.strip_prefix(MATCH_PREFIX).and_then(|s| s.parse::<usize>().ok()) {
        return lower_match(lo, idx, e.span);
    }
    lower_ctor_addr(lo, e, name, args).map(|_| ())
}

// ----------------------------------------------------------- Pattern matching

/// What a case contributes to the key.
struct ArmPlan {
    /// `None` = the case qualifies for EVERY key
    keys: Option<Vec<i128>>,
    /// after the key there are still comparisons to make
    needs_test: bool,
}

fn plan_arm(pat: &Pattern, subject_enum: Option<&EnumDef>) -> ArmPlan {
    match pat {
        Pattern::Wild(_) | Pattern::Bind(..) => ArmPlan { keys: None, needs_test: false },
        Pattern::Int(v, _) => ArmPlan { keys: Some(vec![*v]), needs_test: false },
        Pattern::Bool(b, _) => {
            ArmPlan { keys: Some(vec![if *b { 1 } else { 0 }]), needs_test: false }
        }
        Pattern::Range { lo, hi, inclusive, .. } => {
            let last = if *inclusive { *hi } else { *hi - 1 };
            let extent = last - *lo + 1;
            if extent > 0 && extent <= MAX_RANGE_KEYS {
                ArmPlan {
                    keys: Some((*lo..=last).collect()),
                    needs_test: false,
                }
            } else {
                ArmPlan { keys: None, needs_test: true }
            }
        }
        Pattern::Variant { vname, subs, .. } => {
            let tag = subject_enum
                .and_then(|d| d.variant(vname))
                .map(|v| v.tag)
                .unwrap_or(-1);
            ArmPlan {
                keys: Some(vec![tag]),
                needs_test: subs.iter().any(|s| !s.is_irrefutable()),
            }
        }
    }
}

fn lower_match(lo: &mut Lower, idx: usize, span: Span) -> Option<()> {
    let mi: MatchInfo = match match_info(idx) {
        Some(m) => m,
        None => return lo.ice(span, "unknown pattern match"),
    };
    let sty = lo.ty_of(&mi.subject);
    let def = match &sty {
        Type::Struct(i) => enum_by_struct(*i),
        _ => None,
    };

    // Determine the key value and the address of the subject
    let (base_addr, key, key_fty) = match &def {
        Some(_) => {
            let a = lo.lower_addr(&mi.subject)?;
            let k = lo.load(FTy::U32, a);
            (a, k, FTy::U32)
        }
        None => {
            let ft = match scalar_fty(&sty) {
                Some(f) if f != FTy::Ptr => f,
                _ => return lo.ice(mi.subject.span, "'match' on an unsupported type"),
            };
            let v = lo.lower_expr(&mi.subject)?;
            let (size, align) = lo.size_align(&sty);
            let slot = lo.alloca(size, align);
            lo.store(ft, slot, v);
            (slot, v, ft)
        }
    };
    let start_bb = lo.cur;

    let plans: Vec<ArmPlan> = mi.arms.iter().map(|a| plan_arm(&a.pat, def.as_ref())).collect();
    let arm_body: Vec<BlockId> = mi.arms.iter().map(|_| lo.new_block()).collect();
    let join = lo.new_block();

    // Collect the keys (ascending, free of duplicates)
    let mut keys: BTreeMap<i128, Vec<usize>> = BTreeMap::new();
    for (i, p) in plans.iter().enumerate() {
        if let Some(ks) = &p.keys {
            for k in ks {
                keys.entry(*k).or_default().push(i);
            }
        }
    }

    // Candidate lists per key (source text order)
    let mut cases: Vec<(i128, BlockId)> = Vec::new();
    let default_cands = candidates(&plans, None);
    let default_bb = emit_chain(lo, &mi, &plans, &default_cands, base_addr, key, key_fty, &sty, def.as_ref(), &arm_body, join)?;
    for k in keys.keys().copied().collect::<Vec<_>>() {
        let cands = candidates(&plans, Some(k));
        let bb = emit_chain(lo, &mi, &plans, &cands, base_addr, key, key_fty, &sty, def.as_ref(), &arm_body, default_bb)?;
        cases.push((k, bb));
    }

    if cases.is_empty() {
        lo.f.set_term(start_bb, Term::Br(default_bb));
    } else {
        lo.f.set_term(
            start_bb,
            Term::Switch { val: key, ty: key_fty, cases, default: default_bb },
        );
    }

    // Body of every case
    for (i, arm) in mi.arms.iter().enumerate() {
        lo.cur = arm_body[i];
        lo.enter();
        bind_pattern(lo, base_addr, &arm.pat, &sty, def.as_ref());
        let r = lo.lower_block(&arm.body);
        lo.leave();
        r?;
        if !lo.terminated() {
            lo.set_term(Term::Br(join));
        }
    }
    lo.cur = join;
    Some(())
}

/// Cases that qualify for `key` (or for the catch-all chain).
/// After the first case without a residual check it stops — everything past
/// that is unreachable for this key.
fn candidates(plans: &[ArmPlan], key: Option<i128>) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, p) in plans.iter().enumerate() {
        let fits = match (&p.keys, key) {
            (None, _) => true,
            (Some(ks), Some(k)) => ks.contains(&k),
            (Some(_), None) => false,
        };
        if !fits {
            continue;
        }
        out.push(i);
        if !p.needs_test {
            break;
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn emit_chain(
    lo: &mut Lower,
    mi: &MatchInfo,
    plans: &[ArmPlan],
    cands: &[usize],
    base_addr: Val,
    key: Val,
    key_fty: FTy,
    sty: &Type,
    def: Option<&EnumDef>,
    arm_body: &[BlockId],
    fallthrough: BlockId,
) -> Option<BlockId> {
    let entry = lo.new_block();
    lo.cur = entry;
    for ai in cands {
        let arm = match mi.arms.get(*ai) {
            Some(a) => a,
            None => continue,
        };
        let body = match arm_body.get(*ai) {
            Some(b) => *b,
            None => continue,
        };
        if !plans[*ai].needs_test {
            lo.set_term(Term::Br(body));
            return Some(entry);
        }
        let fail = lo.new_block();
        emit_tests(lo, base_addr, &arm.pat, sty, def, key, key_fty, fail)?;
        lo.set_term(Term::Br(body));
        lo.cur = fail;
    }
    lo.set_term(Term::Br(fallthrough));
    Some(entry)
}

/// Residual checks of a pattern. On success control falls through to block
/// `lo.cur` (after the return), on failure it goes to `fail`.
#[allow(clippy::too_many_arguments)]
fn emit_tests(
    lo: &mut Lower,
    base_addr: Val,
    pat: &Pattern,
    ty: &Type,
    def: Option<&EnumDef>,
    key: Val,
    key_fty: FTy,
    fail: BlockId,
) -> Option<()> {
    match pat {
        Pattern::Wild(_) | Pattern::Bind(..) | Pattern::Int(..) | Pattern::Bool(..) => Some(()),
        Pattern::Range { lo: rlo, hi, inclusive, .. } => {
            let last = if *inclusive { *hi } else { *hi - 1 };
            let c1 = {
                let k = lo.constant(key_fty, *rlo);
                lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Ge, ty: key_fty, a: key, b: k })
            };
            let next = lo.new_block();
            lo.set_term(Term::BrCond { cond: c1, then_bb: next, else_bb: fail });
            lo.cur = next;
            let c2 = {
                let k = lo.constant(key_fty, last);
                lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Le, ty: key_fty, a: key, b: k })
            };
            let next2 = lo.new_block();
            lo.set_term(Term::BrCond { cond: c2, then_bb: next2, else_bb: fail });
            lo.cur = next2;
            Some(())
        }
        Pattern::Variant { vname, subs, span, .. } => {
            let d = match def {
                Some(d) => d.clone(),
                None => return lo.ice(*span, "variant pattern without enum"),
            };
            let v = match d.variant(vname) {
                Some(v) => v.clone(),
                None => return lo.ice(*span, "unknown variant in lowering"),
            };
            let _ = ty;
            for (i, sub) in subs.iter().enumerate() {
                if sub.is_irrefutable() {
                    continue;
                }
                let off = match v.offsets.get(i) {
                    Some(o) => *o,
                    None => return lo.ice(*span, "payload field without offset"),
                };
                let fty = match v.fields.get(i) {
                    Some(t) => t.clone(),
                    None => return lo.ice(*span, "payload field without type"),
                };
                // Layer field access <-> storage location (layout.rs)
                let addr = lo.field_addr_at(base_addr, off);
                emit_sub_test(lo, addr, sub, &fty, fail)?;
            }
            Some(())
        }
    }
}

fn emit_sub_test(
    lo: &mut Lower,
    addr: Val,
    pat: &Pattern,
    ty: &Type,
    fail: BlockId,
) -> Option<()> {
    match pat {
        Pattern::Wild(_) | Pattern::Bind(..) => Some(()),
        Pattern::Int(v, span) => {
            let ft = match scalar_fty(ty) {
                Some(f) => f,
                None => return lo.ice(*span, "number pattern on a non-scalar field"),
            };
            let a = lo.load(ft, addr);
            let b = lo.constant(ft, *v);
            let c = lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Eq, ty: ft, a, b });
            let next = lo.new_block();
            lo.set_term(Term::BrCond { cond: c, then_bb: next, else_bb: fail });
            lo.cur = next;
            Some(())
        }
        Pattern::Bool(v, _) => {
            let a = lo.load(FTy::Bool, addr);
            let b = lo.constant(FTy::Bool, if *v { 1 } else { 0 });
            let c = lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Eq, ty: FTy::Bool, a, b });
            let next = lo.new_block();
            lo.set_term(Term::BrCond { cond: c, then_bb: next, else_bb: fail });
            lo.cur = next;
            Some(())
        }
        Pattern::Range { lo: rlo, hi, inclusive, span } => {
            let ft = match scalar_fty(ty) {
                Some(f) => f,
                None => return lo.ice(*span, "range pattern on a non-scalar field"),
            };
            let last = if *inclusive { *hi } else { *hi - 1 };
            let a = lo.load(ft, addr);
            let k1 = lo.constant(ft, *rlo);
            let c1 = lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Ge, ty: ft, a, b: k1 });
            let next = lo.new_block();
            lo.set_term(Term::BrCond { cond: c1, then_bb: next, else_bb: fail });
            lo.cur = next;
            let k2 = lo.constant(ft, last);
            let c2 = lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Le, ty: ft, a, b: k2 });
            let next2 = lo.new_block();
            lo.set_term(Term::BrCond { cond: c2, then_bb: next2, else_bb: fail });
            lo.cur = next2;
            Some(())
        }
        Pattern::Variant { vname, subs, span, .. } => {
            let sidx = match ty {
                Type::Struct(i) => *i,
                _ => return lo.ice(*span, "variant pattern on a non-enum"),
            };
            let d = match enum_by_struct(sidx) {
                Some(d) => d,
                None => return lo.ice(*span, "variant pattern on a non-enum"),
            };
            let v = match d.variant(vname) {
                Some(v) => v.clone(),
                None => return lo.ice(*span, "unknown variant in lowering"),
            };
            let a = lo.load(FTy::U32, addr);
            let b = lo.constant(FTy::U32, v.tag);
            let c = lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Eq, ty: FTy::U32, a, b });
            let next = lo.new_block();
            lo.set_term(Term::BrCond { cond: c, then_bb: next, else_bb: fail });
            lo.cur = next;
            for (i, sub) in subs.iter().enumerate() {
                if sub.is_irrefutable() {
                    continue;
                }
                let off = match v.offsets.get(i) {
                    Some(o) => *o,
                    None => return lo.ice(*span, "payload field without offset"),
                };
                let ft = match v.fields.get(i) {
                    Some(t) => t.clone(),
                    None => return lo.ice(*span, "payload field without type"),
                };
                // Layer field access <-> storage location (layout.rs)
                let sa = lo.field_addr_at(addr, off);
                emit_sub_test(lo, sa, sub, &ft, fail)?;
            }
            Some(())
        }
    }
}

/// Bindings of a pattern: every binding is the ADDRESS of the matched value
/// (enum values live in memory, bindings are immutable).
fn bind_pattern(lo: &mut Lower, addr: Val, pat: &Pattern, ty: &Type, def: Option<&EnumDef>) {
    match pat {
        Pattern::Wild(_) | Pattern::Int(..) | Pattern::Bool(..) | Pattern::Range { .. } => {}
        Pattern::Bind(name, _) => lo.declare(name, addr),
        Pattern::Variant { vname, subs, .. } => {
            let d = match (def, ty) {
                (Some(d), _) => Some(d.clone()),
                (None, Type::Struct(i)) => enum_by_struct(*i),
                _ => None,
            };
            let d = match d {
                Some(d) => d,
                None => return,
            };
            let v = match d.variant(vname) {
                Some(v) => v.clone(),
                None => return,
            };
            for (i, sub) in subs.iter().enumerate() {
                let off = match v.offsets.get(i) {
                    Some(o) => *o,
                    None => continue,
                };
                let ft = match v.fields.get(i) {
                    Some(t) => t.clone(),
                    None => continue,
                };
                // Layer field access <-> storage location (layout.rs)
                let sa = lo.field_addr_at(addr, off);
                bind_pattern(lo, sa, sub, &ft, None);
            }
        }
    }
}
