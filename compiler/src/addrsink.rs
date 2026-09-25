// SPDX-License-Identifier: MPL-2.0
//! **Round TEMPO 15** -- an address `base + k` is copied in front of each of
//! its memory accesses, right before register allocation (`addrsink`).
//!
//! ## Why
//!
//! x86 has `[reg + disp32]` in every memory operand. The code generator
//! folds `base + k` into the access -- but only when the address has ONE
//! reader standing right behind it (`regalloc.rs::foldable_addresses`).
//! After `cse` an address that is loaded AND stored (`l3_dct3_9`: nine
//! floats from `y`, `y+8`, ... and nine results back to the same places) is
//! one value with two readers, far apart. It gets a register of its own for
//! the whole function and costs an `lea` -- nine registers and nine
//! instructions for what the addressing does for free.
//!
//! ## What is done
//!
//! For a value `d = ptradd base, k` (or a 64-bit `add` of a constant) whose
//! EVERY reader is the address operand of a load or store (scalar or
//! vector), each reader gets its own copy `d' = ptradd base, k` right in
//! front of it, and the original goes. Each copy then has exactly the shape
//! the existing fold wants. The base lives a little longer (up to the last
//! access instead of the last address computation); the addresses do not
//! live at all.
//!
//! That can cost registers elsewhere, so the register allocator runs on
//! BOTH versions and keeps this one only if it leaves no more values in the
//! frame than the original (`regalloc.rs::emit_func_ra`).
//!
//! `FIRN_NO_ADDRSINK=1` switches it off.

use crate::fir::{BinOp, FTy, Func, Inst, Op, Term, Val};
use crate::simd::SimdKind;
use std::collections::HashMap;

/// The copy with sunk addresses, or `None` when there is nothing to sink.
pub fn sink(f: &Func) -> Option<Func> {
    if std::env::var_os("FIRN_NO_ADDRSINK").is_some() || !f.secret.is_empty() || f.constant_time {
        return None;
    }
    let nv = f.val_types.len();
    let mut consts: HashMap<Val, i128> = HashMap::new();
    let mut defs = vec![0u32; nv];
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if let Some(c) = defs.get_mut(d as usize) {
                    *c += 1;
                }
                if let Op::Const(k) = i.op {
                    consts.insert(d, k);
                }
            }
        }
    }
    // candidates: d -> (the defining op to copy, its type)
    let mut cand: HashMap<Val, (Op, FTy)> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            let d = match i.dst {
                Some(d) if defs.get(d as usize) == Some(&1) => d,
                _ => continue,
            };
            let k = match &i.op {
                Op::PtrAdd { off, .. } => consts.get(off).copied(),
                Op::Bin(BinOp::Add, x, y) if i.ty.bits() == 64 => {
                    match (consts.get(x), consts.get(y)) {
                        (None, Some(k)) => Some(*k),
                        (Some(k), None) => Some(*k),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(k) = k {
                // the fold takes displacements 0 ..= i32::MAX (a negative one
                // would stay an `lea` per copy -- worse than one shared)
                let k64 = (k as u64) as i64;
                if (0..=i32::MAX as i64).contains(&k64) {
                    cand.insert(d, (i.op.clone(), i.ty));
                }
            }
        }
    }
    if cand.is_empty() {
        return None;
    }
    // every reader must be a memory access through it; count them
    let mut readers: HashMap<Val, u32> = HashMap::new();
    let mut bad: std::collections::HashSet<Val> = std::collections::HashSet::new();
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in b.insts.iter() {
            buf.clear();
            i.op.uses(&mut buf);
            for u in &buf {
                if !cand.contains_key(u) {
                    continue;
                }
                let ok = match &i.op {
                    Op::Load { addr } => addr == u,
                    Op::Store { addr, val } => addr == u && val != u,
                    Op::Simd { kind: SimdKind::Load, args, .. } => args.len() == 1 && args[0] == *u,
                    Op::Simd { kind: SimdKind::Store | SimdKind::Store64, args, .. } => {
                        args.len() == 2 && args[0] == *u && args[1] != *u
                    }
                    _ => false,
                };
                if ok {
                    *readers.entry(*u).or_insert(0) += 1;
                } else {
                    bad.insert(*u);
                }
            }
        }
        let tv: Option<Val> = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = tv {
            bad.insert(v);
        }
    }
    // Only addresses with two or more readers: a single reader either
    // stands right behind (folded already) or the sinking buys one `lea`
    // for a longer base -- not worth a second allocation.
    let sinkable: std::collections::HashSet<Val> = cand
        .keys()
        .copied()
        .filter(|d| !bad.contains(d) && readers.get(d).copied().unwrap_or(0) >= 2)
        .collect();
    if sinkable.is_empty() {
        return None;
    }
    let mut g = f.clone();
    for bi in 0..g.blocks.len() {
        let old = std::mem::take(&mut g.blocks[bi].insts);
        let mut out: Vec<Inst> = Vec::with_capacity(old.len() + 8);
        for mut i in old {
            if let Some(d) = i.dst {
                if sinkable.contains(&d) {
                    continue; // the original goes; every reader has its copy
                }
            }
            let addr_slot: Option<&mut Val> = match &mut i.op {
                Op::Load { addr } => Some(addr),
                Op::Store { addr, .. } => Some(addr),
                Op::Simd { kind: SimdKind::Load | SimdKind::Store | SimdKind::Store64, args, .. } => {
                    args.get_mut(0)
                }
                _ => None,
            };
            if let Some(a) = addr_slot {
                if sinkable.contains(a) {
                    let (op, ty) = cand[a].clone();
                    let nd = g.new_val_pub(ty);
                    out.push(Inst::like(Some(nd), ty, op, i.loc));
                    *a = nd;
                }
            }
            out.push(i);
        }
        g.blocks[bi].insts = out;
    }
    Some(g)
}
