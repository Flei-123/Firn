// SPDX-License-Identifier: MPL-2.0
//! **Round TEMPO 12 -- a struct on the stack becomes individual cells.**
//!
//! ## Why this pass exists
//!
//! The Huffman reader of the MP3 decoder holds its state in a small
//! struct and passes a pointer to it on to four helpers:
//!
//! ```firn
//! var state: HuffState = HuffState { cache: .., sh: .., next: .. }
//! let l: *mut HuffState = &state
//! ... flush_bits(l, w) ... peek_bits(l, 5) ... check_bits(l)
//! ```
//!
//! After inlining the helpers only an `alloca` is left of it,
//! which is loaded from and written to with fixed offsets. But `mem2reg`
//! promotes only cells that are read and written as a WHOLE
//! -- a `ptradd` on the cell makes it untouchable for it. So
//! `cache`, `sh` and `next` lay in the frame the whole function, and every
//! bit operation of the hottest decoder part went through memory:
//! `mov -0x1cc0(%rbp),%r8d ... mov %r13d,-0x1cc0(%rbp)`. In C exactly
//! this state (`bs_cache`, `bs_sh`, `bs_next` in minimp3) is a handful of
//! local variables and stands in registers.
//!
//! ## What the pass does
//!
//! An `alloca` whose address is used ONLY like this:
//!
//!   * `load`/`store` directly on the cell (offset 0), or
//!   * `p = ptradd cell, K` with constant `K`, and `p` in turn ONLY as the
//!     address of a `load`/`store`,
//!
//! is split into one cell per offset. Each new cell is again an
//! ordinary `alloca` that `mem2reg` promotes in the next round.
//!
//! ## Why this cannot break anything
//!
//! The address never leaves the function (it is neither stored nor
//! passed nor compared -- every such use makes the pass keep its
//! hands off). So nobody but the enumerated accesses can
//! see the memory. It is also required that all accesses to one
//! offset have the same type and that no two fields overlap -- then
//! every access is exactly one field, and a field in a cell of its own
//! behaves like the same field in the struct. A `copymem` to or from the
//! cell (struct assignment as a whole) is a different use and
//! prevents the pass; that is the next step, not this one.
//!
//! Can be switched off with `--no-pass=sroa` or `FIRN_NO_SROA=1`.

use crate::fir::{Func, Op, Term, Val};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) fn split(f: &mut Func) -> usize {
    if std::env::var_os("FIRN_NO_SROA").is_some() {
        return 0;
    }
    // Constants (for the offsets).
    let mut consts: HashMap<Val, i128> = HashMap::new();
    let mut cells: HashMap<Val, (u64, u64)> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            match (&i.op, i.dst) {
                (Op::Const(c), Some(d)) => {
                    consts.insert(d, *c);
                }
                (Op::Alloca { size, align }, Some(d)) => {
                    cells.insert(d, (*size, *align));
                }
                _ => {}
            }
        }
    }
    if cells.is_empty() {
        return 0;
    }
    // ptradd-Ergebnis -> (Zelle, Versatz)
    let mut feldzeiger: HashMap<Val, (Val, i128)> = HashMap::new();
    let mut bad: HashSet<Val> = HashSet::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Op::PtrAdd { base, off }, Some(d)) = (&i.op, i.dst) {
                if cells.contains_key(base) {
                    match consts.get(off) {
                        Some(&k) if k >= 0 => {
                            feldzeiger.insert(d, (*base, k));
                        }
                        _ => {
                            bad.insert(*base);
                        }
                    }
                }
            }
        }
    }
    // Cell -> offset -> type; every other use makes the cell bad.
    let mut fields: HashMap<Val, BTreeMap<i128, crate::fir::FTy>> = HashMap::new();
    let root = |v: Val| -> Option<(Val, i128)> {
        if cells.contains_key(&v) {
            Some((v, 0))
        } else {
            feldzeiger.get(&v).copied()
        }
    };
    let mut buf: Vec<Val> = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                Op::Load { addr } | Op::Store { addr, .. } => {
                    if let Some((z, k)) = root(*addr) {
                        let e = fields.entry(z).or_default();
                        match e.get(&k) {
                            None => {
                                e.insert(k, i.ty);
                            }
                            Some(t) if *t == i.ty => {}
                            _ => {
                                bad.insert(z);
                            }
                        }
                    }
                    if let Op::Store { val, .. } = &i.op {
                        if let Some((z, _)) = root(*val) {
                            bad.insert(z);
                        }
                    }
                }
                Op::PtrAdd { base, .. } if cells.contains_key(base) => {
                    // already classified above; the offset itself is not a
                    // use of a cell
                }
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    for v in &buf {
                        if let Some((z, _)) = root(*v) {
                            bad.insert(z);
                        }
                    }
                }
            }
        }
        let t = match &b.term {
            Term::Ret(Some(v)) => Some(*v),
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            _ => None,
        };
        if let Some(v) = t {
            if let Some((z, _)) = root(v) {
                bad.insert(z);
            }
        }
        // phi entries count as a use
        for i in &b.insts {
            if let Op::Phi { incoming } = &i.op {
                for (_, v) in incoming.iter() {
                    if let Some((z, _)) = root(*v) {
                        bad.insert(z);
                    }
                }
            }
        }
    }
    // Selection: at least one ptradd (otherwise mem2reg can already do it), no
    // overlap, everything inside the cell, no secret value.
    let mut candidates: Vec<Val> = fields
        .iter()
        .filter(|(z, fs)| {
            if bad.contains(z) || f.is_secret(**z) {
                return false;
            }
            if !feldzeiger.values().any(|(w, _)| w == *z) {
                return false;
            }
            let (size, _) = cells[*z];
            let mut end: i128 = 0;
            for (&k, t) in fs.iter() {
                let n = t.bytes() as i128;
                if n == 0 || k < end || k + n > size as i128 {
                    return false;
                }
                end = k + n;
            }
            true
        })
        .map(|(z, _)| *z)
        .collect();
    if candidates.is_empty() {
        return 0;
    }
    // Fixed order (fixed point: two runs must give the same text).
    candidates.sort_unstable();
    let mut neu: HashMap<(Val, i128), Val> = HashMap::new();
    for z in &candidates {
        let (_, align) = cells[z];
        for (&k, t) in fields[z].iter() {
            let n = t.bytes();
            let a = n.min(align).max(1);
            let nv = f.alloca(n, a);
            neu.insert((*z, k), nv);
        }
    }
    let target = |v: Val| -> Option<Val> {
        let (z, k) = if cells.contains_key(&v) {
            (v, 0)
        } else {
            *feldzeiger.get(&v)?
        };
        neu.get(&(z, k)).copied()
    };
    for b in f.blocks.iter_mut() {
        for i in b.insts.iter_mut() {
            match &mut i.op {
                Op::Load { addr } | Op::Store { addr, .. } => {
                    if let Some(nv) = target(*addr) {
                        *addr = nv;
                    }
                }
                _ => {}
            }
        }
    }
    // The old ptradd and the old cell are now dead; `dce` clears them.
    candidates.len()
}
