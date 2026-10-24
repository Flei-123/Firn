// SPDX-License-Identifier: MPL-2.0
//! `loadcse` -- redundant load elimination and store-to-load forwarding with
//! alias facts.
//!
//! The optimiser had no memory model beyond `mem2reg` (stack cells whose
//! address never leaks) and `promote` (loop-carried fields). Here every
//! access gets a LOCATION = (root, constant offset, size):
//!
//! * root `Alloca(v)`: a stack slot of this function;
//! * root `Global(name)`: a `static`;
//! * root `Sym(v)`: any other pointer value (parameter, loaded pointer, ...),
//!   only comparable to itself.
//!
//! Alias facts (the only ones used):
//! * same root: overlap of the byte ranges decides; an unknown (non-constant)
//!   offset overlaps everything of that root;
//! * two different `Alloca` roots, two different `Global` names, `Alloca` vs
//!   `Global`: never alias;
//! * `Alloca` vs `Sym`/`Global`: never alias while the alloca's address does
//!   not escape (only used as the address of a load/store/copy or as the base
//!   of `ptradd`; never stored, passed, cast, merged in a phi, returned);
//! * everything else (Sym vs Sym with different roots, Sym vs Global) may alias.
//!
//! The pass walks the blocks in reverse post order and carries a table
//! "location -> value known to be there" along single-predecessor edges
//! (extended basic blocks). A store kills the aliasing entries and records its
//! value; a load that finds its location (same size and type) is replaced by
//! the recorded value, otherwise it is recorded. Anything with a memory effect
//! the pass does not model (call, syscall, atomic, asm, barrier, copy into an
//! unknown place, ...) clears the table except for non-escaping stack slots
//! the operation cannot touch. Functions with secret values are skipped.
//! Switch: `--no-pass=loadcse`.

use crate::fir::{FTy, Func, Op, Term, Val};
use crate::fx::{HashMap, HashSet};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Root {
    Alloca(Val),
    Global(String),
    Sym(Val),
}

#[derive(Clone, Debug)]
pub(crate) struct Loc {
    pub(crate) root: Root,
    pub(crate) off: Option<i64>,
}

#[derive(Clone)]
pub(crate) struct Entry {
    pub(crate) root: Root,
    pub(crate) off: i64,
    pub(crate) size: u64,
    pub(crate) ty: FTy,
    pub(crate) val: Val,
}

pub(crate) type Table = Vec<Entry>;

const MAX_ENTRIES: usize = 48;

pub(crate) struct Facts {
    loc: HashMap<Val, Loc>,
    escaping: HashSet<Val>,
    taken: Option<crate::gtaken::Taken>,
}

fn size_of(ty: FTy) -> u64 {
    (ty.bits() / 8).max(1) as u64
}

fn analyse(f: &Func) -> Facts {
    let mut consts: HashMap<Val, i128> = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                consts.insert(d, *c);
            }
        }
    }
    let mut loc: HashMap<Val, Loc> = HashMap::default();
    // fixpoint over ptradd chains (block order is not definition order)
    loop {
        let mut grew = false;
        for b in &f.blocks {
            for i in &b.insts {
                let d = match i.dst {
                    Some(d) => d,
                    None => continue,
                };
                if loc.contains_key(&d) {
                    continue;
                }
                match &i.op {
                    Op::Alloca { .. } => {
                        loc.insert(d, Loc { root: Root::Alloca(d), off: Some(0) });
                        grew = true;
                    }
                    Op::GlobalAddr { name } => {
                        loc.insert(d, Loc { root: Root::Global(name.clone()), off: Some(0) });
                        grew = true;
                    }
                    Op::PtrAdd { base, off } => {
                        let b0 = loc.get(base).cloned().unwrap_or(Loc { root: Root::Sym(*base), off: Some(0) });
                        let nl = match (b0.off, consts.get(off)) {
                            (Some(o), Some(&c)) if c.abs() < (1 << 40) => {
                                Loc { root: b0.root, off: Some(o + c as i64) }
                            }
                            _ => Loc { root: b0.root, off: None },
                        };
                        // a base that is not resolved yet and is not a plain
                        // value: wait for the next round
                        if !loc.contains_key(base) && is_defined_by_ptr_op(f, *base) {
                            continue;
                        }
                        loc.insert(d, nl);
                        grew = true;
                    }
                    _ => {}
                }
            }
        }
        if !grew {
            break;
        }
    }
    // escape analysis for allocas
    let mut escaping: HashSet<Val> = HashSet::default();
    let root_of = |v: &Val, loc: &HashMap<Val, Loc>| -> Option<Val> {
        match loc.get(v) {
            Some(Loc { root: Root::Alloca(a), .. }) => Some(*a),
            _ => None,
        }
    };
    let mut buf: Vec<Val> = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                Op::Load { .. } | Op::Alloca { .. } | Op::Const(_) => {}
                Op::Store { val, .. } => {
                    if let Some(a) = root_of(val, &loc) {
                        escaping.insert(a);
                    }
                }
                Op::PtrAdd { off, .. } => {
                    if let Some(a) = root_of(off, &loc) {
                        escaping.insert(a);
                    }
                }
                Op::CopyMem { .. } => {}
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    for v in &buf {
                        if let Some(a) = root_of(v, &loc) {
                            escaping.insert(a);
                        }
                    }
                }
            }
        }
        match &b.term {
            Term::BrCond { cond, .. } => {
                if let Some(a) = root_of(cond, &loc) {
                    escaping.insert(a);
                }
            }
            Term::Switch { val, .. } => {
                if let Some(a) = root_of(val, &loc) {
                    escaping.insert(a);
                }
            }
            Term::Ret(Some(v)) => {
                if let Some(a) = root_of(v, &loc) {
                    escaping.insert(a);
                }
            }
            _ => {}
        }
    }
    Facts { loc, escaping, taken: crate::gtaken::get() }
}

fn is_defined_by_ptr_op(f: &Func, v: Val) -> bool {
    for b in &f.blocks {
        for i in &b.insts {
            if i.dst == Some(v) {
                return matches!(i.op, Op::PtrAdd { .. } | Op::Alloca { .. } | Op::GlobalAddr { .. });
            }
        }
    }
    false
}

impl Facts {
    pub(crate) fn locate(&self, v: Val) -> Loc {
        self.loc.get(&v).cloned().unwrap_or(Loc { root: Root::Sym(v), off: Some(0) })
    }

    /// May an access (`root`, `off`, `size`) overlap an entry?
    fn may_alias(&self, root: &Root, off: Option<i64>, size: u64, e: &Entry) -> bool {
        if *root == e.root {
            return match off {
                None => true,
                Some(o) => o < e.off + e.size as i64 && e.off < o + size as i64,
            };
        }
        self.roots_may_alias(root, &e.root)
    }

    fn roots_may_alias(&self, a: &Root, b: &Root) -> bool {
        match (a, b) {
            (Root::Alloca(x), Root::Alloca(y)) => x == y,
            (Root::Alloca(x), Root::Global(g)) | (Root::Global(g), Root::Alloca(x)) => {
                self.escaping.contains(x) && self.is_taken(g)
            }
            (Root::Alloca(x), _) | (_, Root::Alloca(x)) => self.escaping.contains(x),
            (Root::Global(x), Root::Global(y)) => x == y,
            (Root::Global(g), Root::Sym(_)) | (Root::Sym(_), Root::Global(g)) => self.is_taken(g),
            _ => true,
        }
    }

    fn is_taken(&self, name: &str) -> bool {
        match &self.taken {
            Some(t) => t.contains(name),
            None => true,
        }
    }
}

/// Returns the number of loads replaced.
pub fn run(f: &mut Func) -> usize {
    if f.constant_time || !f.secret.is_empty() || f.blocks.is_empty() {
        return 0;
    }
    // quick exit: no load at all
    if !f.blocks.iter().any(|b| b.insts.iter().any(|i| matches!(i.op, Op::Load { .. }))) {
        return 0;
    }
    let facts = analyse(f);
    let n = f.blocks.len();
    // predecessor counts and the single predecessor
    let mut pred_cnt = vec![0usize; n];
    let mut pred_one = vec![usize::MAX; n];
    let mut pred_list: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (bi, b) in f.blocks.iter().enumerate() {
        for s in b.term.successors() {
            let s = s as usize;
            if s < n {
                pred_cnt[s] += 1;
                pred_one[s] = bi;
                pred_list[s].push(bi);
            }
        }
    }
    // reverse post order from block 0
    let mut order: Vec<usize> = Vec::new();
    {
        let mut seen = vec![false; n];
        let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
        seen[0] = true;
        while let Some(&mut (b, ref mut k)) = stack.last_mut() {
            let succ = f.blocks[b].term.successors();
            if *k < succ.len() {
                let s = succ[*k] as usize;
                *k += 1;
                if s < n && !seen[s] {
                    seen[s] = true;
                    stack.push((s, 0));
                }
            } else {
                order.push(b);
                stack.pop();
            }
        }
        order.reverse();
    }
    let mut out_state: Vec<Option<Table>> = vec![None; n];
    let mut subst: HashMap<Val, Val> = HashMap::default();
    let mut pre = 0usize;
    for &bi in &order {
        let mut tab: Table = if pred_cnt[bi] == 1 && pred_one[bi] != bi {
            out_state[pred_one[bi]].clone().unwrap_or_default()
        } else if pred_cnt[bi] >= 2 {
            let (seeded, inserted) = crate::loadpre::seed(f, bi, &pred_list[bi], &out_state, &facts);
            pre += inserted;
            seeded
        } else {
            Vec::new()
        };
        for i in f.blocks[bi].insts.iter() {
            match &i.op {
                Op::Load { addr } => {
                    let l = facts.locate(*addr);
                    if let (Some(o), Some(d)) = (l.off, i.dst) {
                        let sz = size_of(i.ty);
                        let hit = tab.iter().find(|e| e.root == l.root && e.off == o && e.size == sz && e.ty == i.ty).map(|e| e.val);
                        match hit {
                            Some(v) if f.val_types.get(v as usize) == Some(&i.ty) => {
                                subst.insert(d, v);
                            }
                            _ => {
                                if tab.len() >= MAX_ENTRIES {
                                    tab.remove(0);
                                }
                                tab.push(Entry { root: l.root, off: o, size: sz, ty: i.ty, val: d });
                            }
                        }
                    }
                }
                Op::Store { addr, val } => {
                    let l = facts.locate(*addr);
                    let sz = size_of(i.ty);
                    tab.retain(|e| !facts.may_alias(&l.root, l.off, sz, e));
                    if let Some(o) = l.off {
                        let float_or_vec = i.ty.is_float() || i.ty == FTy::V128;
                        let same_ty = f.val_types.get(*val as usize) == Some(&i.ty);
                        if !float_or_vec && same_ty {
                            if tab.len() >= MAX_ENTRIES {
                                tab.remove(0);
                            }
                            tab.push(Entry { root: l.root, off: o, size: sz, ty: i.ty, val: *val });
                        }
                    }
                }
                Op::CopyMem { dst, size, .. } => {
                    let l = facts.locate(*dst);
                    tab.retain(|e| !facts.may_alias(&l.root, l.off, *size, e));
                }
                op if !has_memory_effect(op) => {}
                _ => {
                    // unmodelled effect: everything an outside party could
                    // touch is gone; non-escaping stack slots survive
                    tab.retain(|e| matches!(&e.root, Root::Alloca(a) if !facts.escaping.contains(a)));
                }
            }
        }
        out_state[bi] = Some(tab);
    }
    if subst.is_empty() {
        return pre;
    }
    // resolve chains (a replaced value may itself have been replaced)
    let keys: Vec<Val> = subst.keys().copied().collect();
    for k in keys {
        let mut v = subst[&k];
        let mut guard = 0;
        while let Some(&nv) = subst.get(&v) {
            v = nv;
            guard += 1;
            if guard > 1000 {
                break;
            }
        }
        subst.insert(k, v);
    }
    crate::mem2reg::replace_uses(f, &subst);
    subst.len() + pre
}

pub(crate) fn has_memory_effect(op: &Op) -> bool {
    !matches!(
        op,
        Op::Const(_)
            | Op::Bin(..)
            | Op::BinWrapSat { .. }
            | Op::Cmp { .. }
            | Op::Un(..)
            | Op::Cast { .. }
            | Op::CheckedBin { .. }
            | Op::CheckedDiv { .. }
            | Op::CheckedCast { .. }
            | Op::CheckedIdx { .. }
            | Op::CheckedIdxV { .. }
            | Op::Alloca { .. }
            | Op::PtrAdd { .. }
            | Op::Select { .. }
            | Op::Phi { .. }
            | Op::Copy { .. }
            | Op::GlobalAddr { .. }
            | Op::Load { .. }
    )
}
