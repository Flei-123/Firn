// SPDX-License-Identifier: MPL-2.0
//! `loadpre` -- partial redundancy elimination of loads of statics at join
//! points, driven by the tables of `loadcse`.
//!
//! `loadcse` carries "location -> value" only along single predecessor edges.
//! At a join, a load of a static at the top of the block is often available
//! on some incoming edges and missing on others (the cold edge went through a
//! call). Here the missing edges get the load at the end of their block
//! (never a critical edge: the predecessor must end in a plain branch) and the
//! join gets a phi of the values. The load in the join then finds the phi in
//! the table, and so does every later load of the same static that no store
//! can touch. Only statics are handled: their address is always valid, so
//! hoisting the load over the pure prefix of the block cannot introduce a
//! fault. No path executes more loads than before.
//! Switch: `FIRN_NO_LOADPRE`.

use crate::fir::{BlockId, FTy, Func, Inst, Op, Term, Val};
use crate::loadcse::{has_memory_effect, Entry, Facts, Root, Table};

fn enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("FIRN_NO_LOADPRE").is_none())
}

struct Cand {
    name: String,
    off: i64,
    size: u64,
    ty: FTy,
}

fn candidates(f: &Func, bi: usize, facts: &Facts) -> Vec<Cand> {
    let mut out: Vec<Cand> = Vec::new();
    for i in &f.blocks[bi].insts {
        match &i.op {
            Op::Phi { .. } => {}
            Op::Load { addr } => {
                let l = facts.locate(*addr);
                let (name, o) = match (&l.root, l.off) {
                    (Root::Global(n), Some(0)) => (n.clone(), 0),
                    _ => continue,
                };
                if i.ty.is_float() || i.ty == FTy::V128 || i.dst.is_none() {
                    continue;
                }
                let size = (i.ty.bits() / 8).max(1) as u64;
                if out.iter().any(|c| c.name == name && c.off == o && c.size == size && c.ty == i.ty) {
                    continue;
                }
                out.push(Cand { name, off: o, size, ty: i.ty });
            }
            op if !has_memory_effect(op) => {}
            _ => break,
        }
    }
    out
}

fn find(f: &Func, tab: &Table, c: &Cand) -> Option<Val> {
    tab.iter()
        .find(|e| matches!(&e.root, Root::Global(n) if *n == c.name) && e.off == c.off && e.size == c.size && e.ty == c.ty)
        .map(|e| e.val)
        .filter(|v| f.val_types.get(*v as usize) == Some(&c.ty))
}

fn same_slot(a: &Entry, b: &Entry) -> bool {
    a.root == b.root && a.off == b.off && a.size == b.size && a.ty == b.ty
}

fn phi_entries(f: &Func, bi: usize, preds: &[usize], out: &[Option<Table>]) -> Vec<Entry> {
    let mut res: Vec<Entry> = Vec::new();
    let first = out[preds[0]].as_ref().unwrap();
    if first.is_empty() {
        return res;
    }
    for i in &f.blocks[bi].insts {
        let (d, incoming) = match (i.dst, &i.op) {
            (Some(d), Op::Phi { incoming }) => (d, incoming),
            _ => break,
        };
        let from = |p: usize| incoming.iter().find(|(b, _)| *b as usize == p).map(|(_, v)| *v);
        let v0 = match from(preds[0]) {
            Some(v) => v,
            None => continue,
        };
        for e in first.iter().filter(|e| e.val == v0 && e.ty == i.ty) {
            let all = preds[1..].iter().all(|&p| {
                let want = from(p);
                out[p].as_ref().unwrap().iter().any(|o| same_slot(o, e) && Some(o.val) == want)
            });
            if all && !res.iter().any(|r| same_slot(r, e)) {
                res.push(Entry { root: e.root.clone(), off: e.off, size: e.size, ty: e.ty, val: d });
            }
        }
    }
    res
}

pub(crate) fn seed(f: &mut Func, bi: usize, preds: &[usize], out: &[Option<Table>], facts: &Facts) -> (Vec<Entry>, usize) {
    let mut res: Vec<Entry> = Vec::new();
    let mut inserted = 0usize;
    if !enabled() || preds.len() < 2 || preds.iter().any(|&p| p == bi || out[p].is_none()) {
        return (res, 0);
    }
    let mut sorted = preds.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() != preds.len() {
        return (res, 0);
    }
    res.extend(phi_entries(f, bi, preds, out));
    for c in candidates(f, bi, facts) {
        let avail: Vec<Option<Val>> = preds.iter().map(|&p| find(f, out[p].as_ref().unwrap(), &c)).collect();
        if avail.iter().all(|a| a.is_none()) {
            continue;
        }
        let insertable = preds
            .iter()
            .zip(&avail)
            .all(|(&p, a)| a.is_some() || matches!(f.blocks[p].term, Term::Br(t) if t as usize == bi));
        if !insertable {
            continue;
        }
        let mut incoming: Vec<(BlockId, Val)> = Vec::new();
        for (&p, a) in preds.iter().zip(&avail) {
            let v = match a {
                Some(v) => *v,
                None => {
                    let addr = f.new_val_pub(FTy::Ptr);
                    let nv = f.new_val_pub(c.ty);
                    let blk = &mut f.blocks[p];
                    blk.insts.push(Inst::new(Some(addr), FTy::Ptr, Op::GlobalAddr { name: c.name.clone() }));
                    blk.insts.push(Inst::new(Some(nv), c.ty, Op::Load { addr }));
                    nv
                }
            };
            incoming.push((p as BlockId, v));
        }
        let phi = f.new_val_pub(c.ty);
        let at = f.blocks[bi].phi_count();
        f.blocks[bi].insts.insert(at, Inst::new(Some(phi), c.ty, Op::Phi { incoming }));
        res.push(Entry { root: Root::Global(c.name.clone()), off: c.off, size: c.size, ty: c.ty, val: phi });
        inserted += 1;
    }
    (res, inserted)
}
