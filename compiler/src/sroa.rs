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
//! behaves like the same field in the struct. A `copymem` between two such
//! cells (struct assignment as a whole) is turned into one load and one
//! store per field, provided both cells qualify and carry the same fields
//! in the copied range. A `copymem` to or from any other memory prevents
//! the pass.
//!
//! Can be switched off with `--no-pass=sroa` or `FIRN_NO_SROA=1`.

use crate::fir::{FTy, Func, Inst, Op, Term, Val};
use crate::fx::{HashMap, HashSet};
use std::collections::BTreeMap;

type Fields = BTreeMap<i128, FTy>;

struct CopyEdge {
    dst: Val,
    dst_off: i128,
    src: Val,
    src_off: i128,
    size: i128,
}

struct Analysis {
    cells: HashMap<Val, (u64, u64)>,
    field_ptrs: HashMap<Val, (Val, i128)>,
    bad: HashSet<Val>,
    fields: HashMap<Val, Fields>,
    edges: Vec<CopyEdge>,
}

pub(crate) fn split(f: &mut Func) -> usize {
    if std::env::var_os("FIRN_NO_SROA").is_some() {
        return 0;
    }
    let Some(mut an) = Analysis::collect(f) else {
        return 0;
    };
    an.scan(f);
    an.propagate();
    let selected = an.select(f);
    if selected.is_empty() {
        return 0;
    }
    an.rewrite(f, &selected);
    selected.len()
}

fn range_fields(fields: &Fields, off: i128, size: i128) -> Vec<(i128, FTy)> {
    fields
        .iter()
        .filter(|(&k, _)| k >= off && k < off + size)
        .map(|(&k, &t)| (k - off, t))
        .collect()
}

impl Analysis {
    fn collect(f: &Func) -> Option<Analysis> {
        let mut consts: HashMap<Val, i128> = HashMap::default();
        let mut cells: HashMap<Val, (u64, u64)> = HashMap::default();
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
            return None;
        }
        let mut field_ptrs: HashMap<Val, (Val, i128)> = HashMap::default();
        let mut bad: HashSet<Val> = HashSet::default();
        for b in &f.blocks {
            for i in &b.insts {
                if let (Op::PtrAdd { base, off }, Some(d)) = (&i.op, i.dst) {
                    if cells.contains_key(base) {
                        match consts.get(off) {
                            Some(&k) if k >= 0 => {
                                field_ptrs.insert(d, (*base, k));
                            }
                            _ => {
                                bad.insert(*base);
                            }
                        }
                    }
                }
            }
        }
        Some(Analysis { cells, field_ptrs, bad, fields: HashMap::default(), edges: Vec::new() })
    }

    fn root(&self, v: Val) -> Option<(Val, i128)> {
        if self.cells.contains_key(&v) {
            Some((v, 0))
        } else {
            self.field_ptrs.get(&v).copied()
        }
    }

    fn mark_bad_root(&mut self, v: Val) {
        if let Some((z, _)) = self.root(v) {
            self.bad.insert(z);
        }
    }

    fn scan(&mut self, f: &Func) {
        let mut buf: Vec<Val> = Vec::new();
        for b in &f.blocks {
            for i in &b.insts {
                match &i.op {
                    Op::Load { addr } => self.note_access(*addr, i.ty),
                    Op::Store { addr, val } => {
                        self.note_access(*addr, i.ty);
                        self.mark_bad_root(*val);
                    }
                    Op::PtrAdd { base, .. } if self.cells.contains_key(base) => {}
                    Op::CopyMem { dst, src, size } => self.note_copy(*dst, *src, *size as i128),
                    other => {
                        buf.clear();
                        other.uses(&mut buf);
                        for v in &buf {
                            self.mark_bad_root(*v);
                        }
                    }
                }
            }
            let term_use = match &b.term {
                Term::Ret(Some(v)) => Some(*v),
                Term::BrCond { cond, .. } => Some(*cond),
                Term::Switch { val, .. } => Some(*val),
                _ => None,
            };
            if let Some(v) = term_use {
                self.mark_bad_root(v);
            }
            for i in &b.insts {
                if let Op::Phi { incoming } = &i.op {
                    for (_, v) in incoming.iter() {
                        self.mark_bad_root(*v);
                    }
                }
            }
        }
    }

    fn note_access(&mut self, addr: Val, ty: FTy) {
        let Some((z, k)) = self.root(addr) else {
            return;
        };
        let known = self.fields.entry(z).or_default().get(&k).copied();
        match known {
            None => {
                self.fields.entry(z).or_default().insert(k, ty);
            }
            Some(t) if t == ty => {}
            _ => {
                self.bad.insert(z);
            }
        }
    }

    fn note_copy(&mut self, dst: Val, src: Val, size: i128) {
        match (self.root(dst), self.root(src)) {
            (Some((d, d_off)), Some((s, s_off))) if d != s => {
                let fits = d_off + size <= self.cells[&d].0 as i128
                    && s_off + size <= self.cells[&s].0 as i128;
                if !fits {
                    self.bad.insert(d);
                    self.bad.insert(s);
                }
                self.edges.push(CopyEdge { dst: d, dst_off: d_off, src: s, src_off: s_off, size });
            }
            _ => {
                self.mark_bad_root(dst);
                self.mark_bad_root(src);
            }
        }
    }

    fn propagate(&mut self) {
        loop {
            let mut changed = false;
            for n in 0..self.edges.len() {
                let (d, d_off, s, s_off, size) = {
                    let e = &self.edges[n];
                    (e.dst, e.dst_off, e.src, e.src_off, e.size)
                };
                changed |= self.push_fields(s, s_off, d, d_off, size);
                changed |= self.push_fields(d, d_off, s, s_off, size);
                if self.bad.contains(&d) != self.bad.contains(&s) {
                    self.bad.insert(d);
                    self.bad.insert(s);
                    changed = true;
                }
            }
            if !changed {
                return;
            }
        }
    }

    fn push_fields(&mut self, from: Val, from_off: i128, to: Val, to_off: i128, size: i128) -> bool {
        let Some(source) = self.fields.get(&from).cloned() else {
            return false;
        };
        let to_size = self.cells[&to].0 as i128;
        let mut changed = false;
        for (&k, &t) in source.iter() {
            let n = t.bytes() as i128;
            if k + n <= from_off || k >= from_off + size {
                continue;
            }
            let mapped = k - from_off + to_off;
            if k < from_off || k + n > from_off + size || mapped + n > to_size {
                changed |= self.bad.insert(from);
                changed |= self.bad.insert(to);
                continue;
            }
            let known = self.fields.entry(to).or_default().get(&mapped).copied();
            match known {
                None => {
                    self.fields.entry(to).or_default().insert(mapped, t);
                    changed = true;
                }
                Some(u) if u == t => {}
                _ => {
                    changed |= self.bad.insert(from);
                    changed |= self.bad.insert(to);
                }
            }
        }
        changed
    }

    fn cell_ok(&self, f: &Func, z: Val, fs: &Fields) -> bool {
        if self.bad.contains(&z) || f.is_secret(z) {
            return false;
        }
        let has_ptr = self.field_ptrs.values().any(|(w, _)| *w == z);
        let has_copy = self.edges.iter().any(|e| e.dst == z || e.src == z);
        if !has_ptr && !has_copy {
            return false;
        }
        let size = self.cells[&z].0 as i128;
        let mut end: i128 = 0;
        for (&k, t) in fs.iter() {
            let n = t.bytes() as i128;
            if n == 0 || k < end || k + n > size {
                return false;
            }
            end = k + n;
        }
        true
    }

    fn edge_consistent(&self, e: &CopyEdge) -> bool {
        let (Some(d), Some(s)) = (self.fields.get(&e.dst), self.fields.get(&e.src)) else {
            return false;
        };
        range_fields(d, e.dst_off, e.size) == range_fields(s, e.src_off, e.size)
    }

    fn select(&self, f: &Func) -> Vec<Val> {
        let mut ok: HashSet<Val> = self
            .fields
            .iter()
            .filter(|(z, fs)| self.cell_ok(f, **z, fs))
            .map(|(z, _)| *z)
            .collect();
        loop {
            let before = ok.len();
            for e in &self.edges {
                let both = ok.contains(&e.dst) && ok.contains(&e.src);
                if !(both && self.edge_consistent(e)) {
                    ok.remove(&e.dst);
                    ok.remove(&e.src);
                }
            }
            if ok.len() == before {
                break;
            }
        }
        let mut selected: Vec<Val> = ok.into_iter().collect();
        selected.sort_unstable();
        selected
    }

    fn rewrite(&self, f: &mut Func, selected: &[Val]) {
        let mut new_cells: HashMap<(Val, i128), Val> = HashMap::default();
        for z in selected {
            let (_, align) = self.cells[z];
            for (&k, t) in self.fields[z].iter() {
                let n = t.bytes();
                new_cells.insert((*z, k), f.alloca(n, n.min(align).max(1)));
            }
        }
        let target = |v: Val| -> Option<Val> {
            let (z, k) = self.root(v)?;
            new_cells.get(&(z, k)).copied()
        };
        for bi in 0..f.blocks.len() {
            let old = std::mem::take(&mut f.blocks[bi].insts);
            let mut out: Vec<Inst> = Vec::with_capacity(old.len());
            for mut i in old {
                match &mut i.op {
                    Op::Load { addr } | Op::Store { addr, .. } => {
                        if let Some(nv) = target(*addr) {
                            *addr = nv;
                        }
                    }
                    Op::CopyMem { dst, src, size } => {
                        let (dst, src, size) = (*dst, *src, *size as i128);
                        if let (Some((d, d_off)), Some((s, s_off))) = (self.root(dst), self.root(src)) {
                            if selected.contains(&d) && selected.contains(&s) {
                                self.expand_copy(f, &mut out, &new_cells, (d, d_off), (s, s_off), size, &i);
                                continue;
                            }
                        }
                    }
                    _ => {}
                }
                out.push(i);
            }
            f.blocks[bi].insts = out;
        }
    }

    fn expand_copy(
        &self,
        f: &mut Func,
        out: &mut Vec<Inst>,
        new_cells: &HashMap<(Val, i128), Val>,
        dst: (Val, i128),
        src: (Val, i128),
        size: i128,
        at: &Inst,
    ) {
        for (rel, t) in range_fields(&self.fields[&src.0], src.1, size) {
            let (Some(&from), Some(&to)) =
                (new_cells.get(&(src.0, src.1 + rel)), new_cells.get(&(dst.0, dst.1 + rel)))
            else {
                continue;
            };
            let v = f.new_val_pub(t);
            out.push(Inst::like(Some(v), t, Op::Load { addr: from }, at.loc));
            out.push(Inst::like(None, t, Op::Store { addr: to, val: v }, at.loc));
        }
    }
}
