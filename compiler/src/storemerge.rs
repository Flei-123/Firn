// SPDX-License-Identifier: MPL-2.0
//! `storemerge` -- merge runs of constant stores into wide stores.
//!
//! Initialising a stack array (`var buf: [u8; 24] = [0, 0, ...]`) lowers to
//! one byte store per element, each with its own address computation. In a
//! straight run of such stores into the same stack slot, every aligned group
//! of 8, 4 or 2 neighbouring bytes that is written exactly once becomes ONE
//! store of the combined constant (little endian; both targets are).
//!
//! Safe because: only stores of constants to `alloca + constant offset`, only
//! when nothing between them reads or writes memory (only constants, pointer
//! arithmetic and plain arithmetic may stand in between), and the wide store
//! is placed where the LAST store of its group stood. Functions with secret
//! values are skipped. Switch: `--no-pass=storemerge`.

use crate::fir::{FTy, Func, Inst, Op, Val};
use crate::fx::HashMap;

struct St {
    /// index of the store instruction in the block
    idx: usize,
    off: u64,
    size: u64,
    value: u64,
    addr: Val,
}

pub fn run(f: &mut Func) -> usize {
    if f.constant_time || !f.secret.is_empty() {
        return 0;
    }
    let mut consts: HashMap<Val, i128> = HashMap::default();
    let mut allocas: HashMap<Val, u64> = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            match (&i.dst, &i.op) {
                (Some(d), Op::Const(c)) => {
                    consts.insert(*d, *c);
                }
                (Some(d), Op::Alloca { size, .. }) => {
                    allocas.insert(*d, *size);
                }
                _ => {}
            }
        }
    }
    if allocas.is_empty() {
        return 0;
    }
    // address value -> (alloca, offset)
    let mut addr_of: HashMap<Val, (Val, u64)> = HashMap::default();
    for (&a, _) in allocas.iter() {
        addr_of.insert(a, (a, 0));
    }
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::PtrAdd { base, off }) = (i.dst, &i.op) {
                if let (Some(&(root, o)), Some(&c)) = (addr_of.get(base), consts.get(off)) {
                    if c >= 0 && (o as i128 + c) < 1 << 30 {
                        addr_of.insert(d, (root, o + c as u64));
                    }
                }
            }
        }
    }
    let mut total = 0usize;
    for bi in 0..f.blocks.len() {
        // collect runs
        let mut runs: Vec<Vec<St>> = Vec::new();
        let mut cur: Vec<St> = Vec::new();
        let mut cur_root: Option<Val> = None;
        for (ii, i) in f.blocks[bi].insts.iter().enumerate() {
            match &i.op {
                Op::Const(_) | Op::PtrAdd { .. } | Op::Alloca { .. } | Op::Bin(..) | Op::Un(..) | Op::Cast { .. } => {}
                Op::Store { addr, val } => {
                    let ok_ty = matches!(i.ty, FTy::I8 | FTy::U8 | FTy::I16 | FTy::U16 | FTy::I32 | FTy::U32 | FTy::I64 | FTy::U64);
                    let info = addr_of.get(addr).copied();
                    let cv = consts.get(val).copied();
                    match (ok_ty, info, cv) {
                        (true, Some((root, off)), Some(c)) if cur_root.is_none() || cur_root == Some(root) => {
                            let size = (i.ty.bits() / 8) as u64;
                            let mask: u128 = if size == 8 { u64::MAX as u128 } else { (1u128 << (size * 8)) - 1 };
                            cur_root = Some(root);
                            cur.push(St { idx: ii, off, size, value: ((c as u128) & mask) as u64, addr: *addr });
                        }
                        _ => {
                            if !cur.is_empty() {
                                runs.push(std::mem::take(&mut cur));
                            }
                            cur_root = None;
                        }
                    }
                }
                _ => {
                    if !cur.is_empty() {
                        runs.push(std::mem::take(&mut cur));
                    }
                    cur_root = None;
                }
            }
        }
        if !cur.is_empty() {
            runs.push(cur);
        }
        // plan: per run, windows to merge
        struct Plan {
            last_idx: usize,
            remove: Vec<usize>,
            addr: Val,
            width: u64,
            value: u64,
        }
        let mut plans: Vec<Plan> = Vec::new();
        for r in &runs {
            if r.len() < 2 {
                continue;
            }
            // reject overlapping stores in the run altogether
            let mut sorted: Vec<&St> = r.iter().collect();
            sorted.sort_by_key(|s| s.off);
            if sorted.windows(2).any(|w| w[0].off + w[0].size > w[1].off) {
                continue;
            }
            let mut used = vec![false; sorted.len()];
            for width in [8u64, 4, 2] {
                let mut k = 0;
                while k < sorted.len() {
                    if used[k] || sorted[k].size >= width || sorted[k].off % width != 0 {
                        k += 1;
                        continue;
                    }
                    // gather stores covering [off, off+width) exactly
                    let base = sorted[k].off;
                    let mut j = k;
                    let mut end = base;
                    let mut ok = true;
                    let mut members = Vec::new();
                    while j < sorted.len() && end < base + width {
                        if used[j] || sorted[j].off != end || sorted[j].size >= width {
                            ok = false;
                            break;
                        }
                        end += sorted[j].size;
                        members.push(j);
                        j += 1;
                    }
                    if ok && end == base + width && members.len() >= 2 {
                        let mut value = 0u64;
                        let mut last_idx = 0;
                        let mut remove = Vec::new();
                        for &m in &members {
                            value |= sorted[m].value << ((sorted[m].off - base) * 8);
                            last_idx = last_idx.max(sorted[m].idx);
                            remove.push(sorted[m].idx);
                            used[m] = true;
                        }
                        plans.push(Plan { last_idx, remove, addr: sorted[k].addr, width, value });
                        k = j;
                    } else {
                        k += 1;
                    }
                }
            }
        }
        if plans.is_empty() {
            continue;
        }
        // apply in one rebuild pass (indices stay valid)
        let mut replace: HashMap<usize, (Inst, Inst)> = HashMap::default();
        let mut removed: crate::fx::HashSet<usize> = crate::fx::HashSet::default();
        for p in plans {
            let ty = match p.width {
                8 => FTy::U64,
                4 => FTy::U32,
                _ => FTy::U16,
            };
            let cv = f.new_val_pub(ty);
            let loc = f.blocks[bi].insts[p.last_idx].loc;
            let cinst = Inst::like(Some(cv), ty, Op::Const(p.value as i128), loc);
            let sinst = Inst::like(None, ty, Op::Store { addr: p.addr, val: cv }, loc);
            replace.insert(p.last_idx, (cinst, sinst));
            for x in p.remove {
                if x != p.last_idx {
                    removed.insert(x);
                }
            }
            total += 1;
        }
        let old = std::mem::take(&mut f.blocks[bi].insts);
        let mut out = Vec::with_capacity(old.len());
        for (ii, inst) in old.into_iter().enumerate() {
            if removed.contains(&ii) {
                continue;
            }
            if let Some((c, s)) = replace.remove(&ii) {
                out.push(c);
                out.push(s);
            } else {
                out.push(inst);
            }
        }
        f.blocks[bi].insts = out;
    }
    total
}
