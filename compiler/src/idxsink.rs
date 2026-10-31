// SPDX-License-Identifier: MPL-2.0
use crate::chainsink::{offset_base, scaled_index};
use crate::fir::{BinOp, Func, Inst, Op};
use crate::fx::{HashMap, HashSet};

pub fn sink(f: &Func) -> Option<Func> {
    if std::env::var_os("FIRN_NO_IDXSINK").is_some() || !f.secret.is_empty() || f.constant_time {
        return None;
    }
    let uses = crate::usecount::counts(f);
    let mut consts: HashMap<crate::fir::Val, i128> = HashMap::default();
    let mut defs = vec![0u32; f.val_types.len()];
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if let Some(c) = defs.get_mut(d as usize) {
                    *c += 1;
                }
                if let Op::Const(c) = &i.op {
                    consts.insert(d, *c);
                }
            }
        }
    }
    let single_def = |v: crate::fir::Val| defs.get(v as usize).copied() == Some(1);
    let single_use = |v: crate::fir::Val| uses.get(v as usize).copied() == Some(1);
    let mut out = f.clone();
    let mut changed = false;
    for (bi, b) in f.blocks.iter().enumerate() {
        let mut def_at: HashMap<crate::fir::Val, usize> = HashMap::default();
        let mut readers: HashMap<crate::fir::Val, Vec<usize>> = HashMap::default();
        for (ii, i) in b.insts.iter().enumerate() {
            if let Some(d) = i.dst {
                def_at.insert(d, ii);
            }
            match &i.op {
                Op::Load { addr } => readers.entry(*addr).or_default().push(ii),
                Op::Store { addr, val } if addr != val => readers.entry(*addr).or_default().push(ii),
                _ => {}
            }
        }
        let mut removed: HashSet<usize> = HashSet::default();
        let mut copies: HashMap<usize, Vec<Vec<usize>>> = HashMap::default();
        for (ii, i) in b.insts.iter().enumerate() {
            let d = match i.dst {
                Some(d) if single_def(d) && i.ty.bits() == 64 => d,
                _ => continue,
            };
            let off = match &i.op {
                Op::PtrAdd { off, .. } => *off,
                Op::Bin(BinOp::Add, _, y) => *y,
                _ => continue,
            };
            if consts.contains_key(&off) || removed.contains(&ii) {
                continue;
            }
            let rs = match readers.get(&d) {
                Some(rs) if rs.len() >= 2 && rs.len() as u32 == uses.get(d as usize).copied().unwrap_or(0) => rs,
                _ => continue,
            };
            if rs.iter().any(|r| *r <= ii) {
                continue;
            }
            let mut chain = vec![ii];
            if let Some(&j) = def_at.get(&off) {
                let jo = &b.insts[j];
                if j < ii && single_def(off) && single_use(off) && jo.ty.bits() == 64 && !removed.contains(&j) {
                    let scaled = scaled_index(&jo.op, &consts);
                    let plain = if scaled.is_none() { offset_base(&jo.op, &consts) } else { None };
                    if scaled.is_some() || plain.is_some() {
                        chain.insert(0, j);
                        if let Some(xv) = scaled {
                            if let Some(&k) = def_at.get(&xv) {
                                let ko = &b.insts[k];
                                if k < j
                                    && single_def(xv)
                                    && single_use(xv)
                                    && ko.ty.bits() == 64
                                    && offset_base(&ko.op, &consts).is_some()
                                    && !removed.contains(&k)
                                {
                                    chain.insert(0, k);
                                }
                            }
                        }
                    }
                }
            }
            for c in &chain {
                removed.insert(*c);
            }
            for r in rs {
                copies.entry(*r).or_default().push(chain.clone());
            }
        }
        if removed.is_empty() {
            continue;
        }
        let mut insts: Vec<Inst> = Vec::with_capacity(b.insts.len() + removed.len());
        for (ii, i) in b.insts.iter().enumerate() {
            if removed.contains(&ii) {
                continue;
            }
            if let Some(lists) = copies.get(&ii) {
                let mut remap: HashMap<crate::fir::Val, crate::fir::Val> = HashMap::default();
                for chain in lists {
                    for c in chain {
                        let src = &b.insts[*c];
                        let nd = out.new_val_pub(src.ty);
                        let mut op = src.op.clone();
                        op.for_each_use_mut(|v| {
                            if let Some(n) = remap.get(v) {
                                *v = *n;
                            }
                        });
                        remap.insert(src.dst.unwrap(), nd);
                        insts.push(Inst::like(Some(nd), src.ty, op, i.loc));
                    }
                }
                let mut ni = i.clone();
                ni.op.for_each_use_mut(|v| {
                    if let Some(n) = remap.get(v) {
                        *v = *n;
                    }
                });
                insts.push(ni);
            } else {
                insts.push(i.clone());
            }
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
