use super::flow::{Borrows, CellVal, Flow};
use super::index::{term_uses, Bits, Index};
use super::slots::Slots;
use crate::fir::{Func, Inst, Op, Val};
use crate::fx::HashMap;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Handle {
    Val(Val),
    Cell(usize),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Free {
    pub block: usize,
    pub pos: usize,
    pub handle: Handle,
}

struct Items {
    val: HashMap<Val, usize>,
    cell: HashMap<usize, usize>,
    n: usize,
}

struct Ctx<'a> {
    f: &'a Func,
    ix: &'a Index,
    sl: &'a Slots,
    seed: usize,
    site: usize,
    items: Items,
}

pub(crate) fn deaths(f: &Func, ix: &Index, sl: &Slots, fl: &mut Flow, br: &dyn Borrows) -> Vec<Free> {
    let mut out = Vec::new();
    for k in 0..fl.sites.len() {
        let seed = fl.seed_of_site(k);
        if fl.escaped.get(seed) {
            continue;
        }
        if let Some(v) = site_deaths(f, ix, sl, fl, br, k) {
            out.extend(v);
        }
    }
    out
}

fn items_for(f: &Func, sl: &Slots, fl: &Flow, seed: usize, site: usize) -> Items {
    let mut val = HashMap::default();
    let mut n = 0;
    for (v, m) in fl.may.iter().enumerate() {
        if m.get(seed) {
            val.insert(v as Val, n);
            n += 1;
        }
    }
    let mut cell = HashMap::default();
    cell.insert(fl.sites[site].cell_p, n);
    n += 1;
    for b in f.blocks.iter() {
        for inst in b.insts.iter() {
            if let Op::Store { addr, val: sv } = &inst.op {
                if let Some(a) = sl.access(*addr, inst.ty) {
                    if fl.may[*sv as usize].get(seed) && !cell.contains_key(&a.cell) {
                        cell.insert(a.cell, n);
                        n += 1;
                    }
                }
            }
        }
    }
    Items { val, cell, n }
}

impl<'a> Ctx<'a> {
    fn back(&self, fl: &Flow, inst: &Inst, at: (usize, usize), live: &mut Bits) {
        if let Some(d) = inst.dst {
            if let Some(&i) = self.items.val.get(&d) {
                live.clear(i);
            }
        }
        match &inst.op {
            Op::Store { addr, .. } => {
                if let Some(a) = self.sl.access(*addr, inst.ty) {
                    if a.strong {
                        if let Some(&i) = self.items.cell.get(&a.cell) {
                            live.clear(i);
                        }
                    }
                }
            }
            Op::Load { addr } => {
                if let Some(a) = self.sl.access(*addr, inst.ty) {
                    if let Some(&i) = self.items.cell.get(&a.cell) {
                        live.set(i);
                    }
                }
            }
            Op::Call { .. } => {
                if let Some(&k) = fl.site_at.get(&at) {
                    for c in [fl.sites[k].cell_p, fl.sites[k].cell_n] {
                        if let Some(&i) = self.items.cell.get(&c) {
                            live.clear(i);
                        }
                    }
                }
            }
            _ => {}
        }
        if matches!(inst.op, Op::Phi { .. }) {
            return;
        }
        let mut u = Vec::new();
        inst.op.uses(&mut u);
        for v in u {
            if let Some(&i) = self.items.val.get(&v) {
                live.set(i);
            }
        }
    }

    fn carried(&self, live: &Bits, cells: &[CellVal]) -> bool {
        if self.items.val.values().any(|&i| live.get(i)) {
            return true;
        }
        self.items.cell.iter().any(|(&c, &i)| live.get(i) && cells[c].may.get(self.seed))
    }

    fn must_cell(&self, cells: &[CellVal], c: usize) -> bool {
        let x = &cells[c];
        !x.derived && x.may.count() == 1 && x.may.get(self.seed)
    }

    fn live_in(&self, fl: &Flow, bi: usize, out: &Bits) -> Bits {
        let mut live = out.clone();
        let b = &self.f.blocks[bi];
        let mut u = Vec::new();
        term_uses(&b.term, &mut u);
        for v in u {
            if let Some(&i) = self.items.val.get(&v) {
                live.set(i);
            }
        }
        for (ii, inst) in b.insts.iter().enumerate().rev() {
            self.back(fl, inst, (bi, ii), &mut live);
        }
        live
    }

    fn live_out(&self, bi: usize, ins: &[Bits]) -> Bits {
        let mut out = Bits::new(self.items.n);
        let me = self.f.blocks[bi].id;
        for &s in self.ix.succs[bi].iter() {
            out.union(&ins[s]);
            for inst in self.f.blocks[s].insts.iter() {
                if let Op::Phi { incoming } = &inst.op {
                    for (p, v) in incoming.iter() {
                        if *p == me {
                            if let Some(&i) = self.items.val.get(v) {
                                out.set(i);
                            }
                        }
                    }
                } else {
                    break;
                }
            }
        }
        out
    }
}

fn site_deaths(f: &Func, ix: &Index, sl: &Slots, fl: &mut Flow, br: &dyn Borrows, site: usize) -> Option<Vec<Free>> {
    let seed = fl.seed_of_site(site);
    let items = items_for(f, sl, fl, seed, site);
    let cx = Ctx { f, ix, sl, seed, site, items };
    let nb = f.blocks.len();
    let mut ins = vec![Bits::new(cx.items.n); nb];
    let mut outs = vec![Bits::new(cx.items.n); nb];
    let mut rounds = 0;
    loop {
        rounds += 1;
        let mut ch = false;
        for &bi in ix.rpo.iter().rev() {
            let o = cx.live_out(bi, &ins);
            let i = cx.live_in(fl, bi, &o);
            if i != ins[bi] || o != outs[bi] {
                ins[bi] = i;
                outs[bi] = o;
                ch = true;
            }
        }
        if !ch {
            break;
        }
        if rounds > 500 {
            return None;
        }
    }
    let mut frees = Vec::new();
    let mut ends: Vec<Option<Vec<CellVal>>> = vec![None; nb];
    for &bi in ix.rpo.iter() {
        let cin = fl.cin[bi].clone()?;
        let b = &f.blocks[bi];
        let mut states: Vec<Vec<CellVal>> = Vec::with_capacity(b.insts.len() + 1);
        let mut cur = cin;
        states.push(cur.clone());
        for (ii, inst) in b.insts.iter().enumerate() {
            fl.transfer(f, sl, br, &mut cur, inst, (bi, ii), false);
            states.push(cur.clone());
        }
        ends[bi] = Some(cur);
        let mut live = outs[bi].clone();
        let mut u = Vec::new();
        term_uses(&b.term, &mut u);
        for v in u {
            if let Some(&i) = cx.items.val.get(&v) {
                live.set(i);
            }
        }
        let pc = b.phi_count();
        for ii in (pc..b.insts.len()).rev() {
            let inst = &b.insts[ii];
            let after = cx.carried(&live, &states[ii + 1]);
            let mut before_live = live.clone();
            cx.back(fl, inst, (bi, ii), &mut before_live);
            let before = cx.carried(&before_live, &states[ii]);
            let is_site = fl.site_at.get(&(bi, ii)) == Some(&site);
            if is_site && before {
                return None;
            }
            let defines = is_site || inst.dst.map_or(false, |d| cx.items.val.contains_key(&d));
            if (before || defines) && !after {
                if let Some(h) = handle(&cx, fl, inst, is_site, &states[ii + 1]) {
                    frees.push(Free { block: bi, pos: ii + 1, handle: h });
                }
            }
            live = before_live;
        }
    }
    for &bi in ix.rpo.iter() {
        let b = &f.blocks[bi];
        if ix.preds[bi].len() != 1 || b.has_phi() {
            continue;
        }
        let p = ix.preds[bi][0];
        let (Some(pend), Some(cin)) = (ends[p].as_ref(), fl.cin[bi].as_ref()) else { continue };
        if !cx.carried(&outs[p], pend) || cx.carried(&ins[bi], cin) {
            continue;
        }
        let mut h = None;
        for (&v, &i) in cx.items.val.iter() {
            if outs[p].get(i) && fl.must(v) == Some(seed) {
                h = Some(Handle::Val(v));
                break;
            }
        }
        if h.is_none() {
            for (&c, &i) in cx.items.cell.iter() {
                if outs[p].get(i) && cx.must_cell(pend, c) {
                    h = Some(Handle::Cell(c));
                    break;
                }
            }
        }
        if let Some(h) = h {
            frees.push(Free { block: bi, pos: 0, handle: h });
        }
    }
    let _ = cx.site;
    Some(frees)
}

fn handle(cx: &Ctx, fl: &Flow, inst: &Inst, is_site: bool, after: &[CellVal]) -> Option<Handle> {
    if is_site {
        let c = fl.sites[cx.site].cell_p;
        if cx.must_cell(after, c) {
            return Some(Handle::Cell(c));
        }
        return None;
    }
    let mut u = Vec::new();
    inst.op.uses(&mut u);
    if let Some(d) = inst.dst {
        u.push(d);
    }
    u.into_iter().find(|v| fl.must(*v) == Some(cx.seed)).map(Handle::Val)
}
