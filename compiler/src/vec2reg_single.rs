// SPDX-License-Identifier: MPL-2.0
//! A sixteen octet cell that is written ONCE as a vector and read as a whole
//! vector or as eight octet halves in other blocks (a loop-invariant `v128`
//! struct field behind an ABI that passes sixteen octet structs as two words).
//! The single store has to dominate every read, then every read is the
//! stored value (`GetU64` for a half) and the store disappears.

use crate::fir::{FTy, Func, Inst, Op, Term, Val};
use crate::fx::HashMap;
use crate::mem2reg::{dominators, Dom};
use crate::simd::SimdKind;

enum Read {
    Whole,
    Half(u8),
}

struct Uses {
    store: (usize, usize, Val),
    reads: Vec<(usize, usize, Read)>,
}

fn lane_map(f: &Func, cell: Val, consts: &HashMap<Val, i128>) -> Option<HashMap<Val, u8>> {
    let mut lane_of: HashMap<Val, u8> = HashMap::default();
    lane_of.insert(cell, 0);
    for b in &f.blocks {
        for i in &b.insts {
            if let (Op::PtrAdd { base, off }, Some(d)) = (&i.op, i.dst) {
                if *base == cell {
                    match consts.get(off) {
                        Some(&k) if k == 0 || k == 8 => {
                            lane_of.insert(d, k as u8);
                        }
                        _ => return None,
                    }
                }
            }
        }
    }
    Some(lane_of)
}

fn term_uses_cell(f: &Func, lane_of: &HashMap<Val, u8>) -> bool {
    f.blocks.iter().any(|b| match &b.term {
        Term::BrCond { cond, .. } => lane_of.contains_key(cond),
        Term::Switch { val, .. } => lane_of.contains_key(val),
        Term::Ret(Some(v)) => lane_of.contains_key(v),
        _ => false,
    })
}

fn classify(f: &Func, lane_of: &HashMap<Val, u8>) -> Option<Uses> {
    let mut store: Option<(usize, usize, Val)> = None;
    let mut reads: Vec<(usize, usize, Read)> = Vec::new();
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            if let Op::PtrAdd { base, .. } = &i.op {
                if lane_of.contains_key(base) {
                    continue;
                }
            }
            buf.clear();
            i.op.uses(&mut buf);
            if !buf.iter().any(|u| lane_of.contains_key(u)) {
                continue;
            }
            match &i.op {
                Op::Store { addr, val } if lane_of.get(addr) == Some(&0) && f.val_ty(*val) == FTy::V128 => {
                    if lane_of.contains_key(val) || store.replace((bi, ii, *val)).is_some() {
                        return None;
                    }
                }
                Op::Simd { kind: SimdKind::Store, args, .. }
                    if args.len() == 2 && lane_of.get(&args[0]) == Some(&0) =>
                {
                    if lane_of.contains_key(&args[1]) || store.replace((bi, ii, args[1])).is_some() {
                        return None;
                    }
                }
                Op::Load { addr } if i.ty == FTy::V128 && lane_of.get(addr) == Some(&0) => {
                    reads.push((bi, ii, Read::Whole));
                }
                Op::Simd { kind: SimdKind::Load, args, .. }
                    if args.len() == 1 && lane_of.get(&args[0]) == Some(&0) =>
                {
                    reads.push((bi, ii, Read::Whole));
                }
                Op::Load { addr } if matches!(i.ty, FTy::I64 | FTy::U64) && lane_of.contains_key(addr) => {
                    reads.push((bi, ii, Read::Half(lane_of[addr] / 8)));
                }
                _ => return None,
            }
        }
    }
    Some(Uses { store: store?, reads })
}

fn dominated(uses: &Uses, dom: &Dom) -> bool {
    let (sb, si, _) = uses.store;
    uses.reads.iter().all(|(rb, ri, _)| if *rb == sb { *ri > si } else { dom.has(*rb, sb) })
}

pub(crate) fn promote(f: &mut Func, cell: Val, consts: &HashMap<Val, i128>) -> bool {
    let Some(lane_of) = lane_map(f, cell, consts) else {
        return false;
    };
    if term_uses_cell(f, &lane_of) {
        return false;
    }
    let Some(uses) = classify(f, &lane_of) else {
        return false;
    };
    if uses.reads.is_empty() || !dominated(&uses, &dominators(f)) {
        return false;
    }
    rewrite(f, &uses);
    true
}

fn rewrite(f: &mut Func, uses: &Uses) {
    let value = uses.store.2;
    let mut map: HashMap<Val, Val> = HashMap::default();
    let mut halves: HashMap<(usize, usize), u8> = HashMap::default();
    for (bi, ii, read) in &uses.reads {
        match read {
            Read::Whole => {
                if let Some(d) = f.blocks[*bi].insts[*ii].dst {
                    map.insert(d, value);
                }
            }
            Read::Half(h) => {
                halves.insert((*bi, *ii), *h);
            }
        }
    }
    for bi in 0..f.blocks.len() {
        let old = std::mem::take(&mut f.blocks[bi].insts);
        let mut out: Vec<Inst> = Vec::with_capacity(old.len());
        for (ii, mut i) in old.into_iter().enumerate() {
            if (bi, ii) == (uses.store.0, uses.store.1) {
                continue;
            }
            if let Some(h) = halves.get(&(bi, ii)) {
                i.op = Op::Simd { kind: SimdKind::GetU64, args: vec![value], imm: *h };
            }
            out.push(i);
        }
        f.blocks[bi].insts = out;
    }
    crate::mem2reg::replace_uses(f, &map);
}
