// SPDX-License-Identifier: MPL-2.0
//! **GAPS — index chains move right in front of their only access.**
//!
//! `regalloc.rs::foldable_addresses` folds `base + (y + c) * F` into one
//! operand only when the whole chain stands directly in front of the load or
//! store that reads it. A store at the end of a long block (`W[t] = ...`
//! after a dozen loads) keeps its `mul` + `ptradd` at the top, so they cost a
//! register and three instructions. Here every such chain (`add/sub const`,
//! `mul/shl const`, `ptradd/add`) with a single reader that is the address of
//! a later load or store of the same block is moved down in front of it.
//! Pure instructions only, operands stay available. The register allocator
//! keeps the result only when it leaves no more values in the frame
//! (`addrsink::sink` is the single caller). `FIRN_NO_CHAINSINK=1` switches it off.

use crate::fir::{BinOp, Func, Inst, Op, Val};
use crate::fx::{HashMap, HashSet};

fn scaled_index(op: &Op, consts: &HashMap<Val, i128>) -> Option<Val> {
    match op {
        Op::Bin(BinOp::Shl, x, k) => match consts.get(k) {
            Some(c) if (0..=3).contains(c) => Some(*x),
            _ => None,
        },
        Op::Bin(BinOp::Mul, x, k) => match consts.get(k) {
            Some(c) if [1, 2, 4, 8].contains(c) => Some(*x),
            _ => None,
        },
        _ => None,
    }
}

fn offset_base(op: &Op, consts: &HashMap<Val, i128>) -> Option<Val> {
    match op {
        Op::Bin(BinOp::Add | BinOp::Sub, y, c) if consts.contains_key(c) && !consts.contains_key(y) => Some(*y),
        _ => None,
    }
}

pub fn sink(f: &Func) -> Option<Func> {
    if std::env::var_os("FIRN_NO_CHAINSINK").is_some() || !f.secret.is_empty() || f.constant_time {
        return None;
    }
    let uses = crate::usecount::counts(f);
    let mut consts: HashMap<Val, i128> = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                consts.insert(d, *c);
            }
        }
    }
    let single = |v: Val| uses.get(v as usize).copied() == Some(1);
    let mut out = f.clone();
    let mut changed = false;
    for (bi, b) in f.blocks.iter().enumerate() {
        let mut def_at: HashMap<Val, usize> = HashMap::default();
        let mut reader_at: HashMap<Val, usize> = HashMap::default();
        for (ii, i) in b.insts.iter().enumerate() {
            if let Some(d) = i.dst {
                def_at.insert(d, ii);
            }
            match &i.op {
                Op::Load { addr } => {
                    reader_at.insert(*addr, ii);
                }
                Op::Store { addr, val } if addr != val => {
                    reader_at.insert(*addr, ii);
                }
                _ => {}
            }
        }
        let mut moved: HashSet<usize> = HashSet::default();
        let mut attach: HashMap<usize, Vec<usize>> = HashMap::default();
        for (ii, i) in b.insts.iter().enumerate() {
            let d = match i.dst {
                Some(d) if single(d) && i.ty.bits() == 64 => d,
                _ => continue,
            };
            let (base, off) = match &i.op {
                Op::PtrAdd { base, off } => (*base, *off),
                Op::Bin(BinOp::Add, x, y) => (*x, *y),
                _ => continue,
            };
            let r = match reader_at.get(&d) {
                Some(r) if *r > ii + 1 => *r,
                _ => continue,
            };
            if moved.contains(&ii) {
                continue;
            }
            let mut chain = vec![ii];
            let idx_val = if consts.contains_key(&off) { None } else { Some(off) };
            let _ = base;
            if let Some(o) = idx_val {
                if let Some(&j) = def_at.get(&o) {
                    if j < ii && single(o) && b.insts[j].ty.bits() == 64 && !moved.contains(&j) {
                        let (x, is_scale) = match scaled_index(&b.insts[j].op, &consts) {
                            Some(x) => (Some(x), true),
                            None => (offset_base(&b.insts[j].op, &consts), false),
                        };
                        if x.is_some() {
                            chain.insert(0, j);
                            if is_scale {
                                if let Some(xv) = x {
                                    if let Some(&k) = def_at.get(&xv) {
                                        if k < j && single(xv) && b.insts[k].ty.bits() == 64 && offset_base(&b.insts[k].op, &consts).is_some() && !moved.contains(&k) {
                                            chain.insert(0, k);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for c in &chain {
                moved.insert(*c);
            }
            attach.entry(r).or_default().extend(chain);
        }
        if moved.is_empty() {
            continue;
        }
        let mut insts: Vec<Inst> = Vec::with_capacity(b.insts.len());
        for (ii, i) in b.insts.iter().enumerate() {
            if moved.contains(&ii) {
                continue;
            }
            if let Some(list) = attach.get(&ii) {
                for c in list {
                    insts.push(b.insts[*c].clone());
                }
            }
            insts.push(i.clone());
        }
        out.blocks[bi].insts = insts;
        changed = true;
    }
    if changed {
        Some(out)
    } else {
        None
    }
}
